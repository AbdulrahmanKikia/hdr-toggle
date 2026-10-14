//! HDR (advanced color) query and toggle for the primary display.
//!
//! Uses the documented DisplayConfig API. If that fails or the display does not
//! report HDR support, falls back to the Win + Alt + B system shortcut.

use std::mem::size_of;

use anyhow::{anyhow, ensure, Context, Result};
use windows::core::HRESULT;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes,
    QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE,
    DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
    DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID, WIN32_ERROR};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_B, VK_LWIN, VK_MENU,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdrState {
    On,
    Off,
}

impl HdrState {
    fn from_enabled(enabled: bool) -> Self {
        if enabled {
            HdrState::On
        } else {
            HdrState::Off
        }
    }

    pub fn is_on(self) -> bool {
        self == HdrState::On
    }

    fn flipped(self) -> Self {
        Self::from_enabled(!self.is_on())
    }
}

/// How a toggle was carried out.
#[derive(Debug)]
pub enum Source {
    /// The DisplayConfig API was used and the resulting state was read back.
    DisplayConfig,
    /// Win + Alt + B was sent; the state is assumed. `reason` says why the API path was skipped.
    Fallback { reason: anyhow::Error },
}

// The windows crate only exposes DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO behind the
// Win32_Graphics_Gdi feature, and both structs wrap their bitfield in a union that
// needs unsafe to read. These are the minimal equivalents of the documented wingdi.h
// layouts, with the bitfield as a plain u32.

/// DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO
#[repr(C)]
struct GetAdvancedColorInfo {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
    _color_encoding: i32,
    _bits_per_color_channel: u32,
}

/// DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE
#[repr(C)]
struct SetAdvancedColorState {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
}

const ADVANCED_COLOR_SUPPORTED: u32 = 1 << 0;
const ADVANCED_COLOR_ENABLED: u32 = 1 << 1;
const WIDE_COLOR_ENFORCED: u32 = 1 << 2;
const ADVANCED_COLOR_FORCE_DISABLED: u32 = 1 << 3;
const ENABLE_ADVANCED_COLOR: u32 = 1 << 0;

/// A display target as identified by the DisplayConfig API.
struct Target {
    adapter_id: LUID,
    id: u32,
}

/// HDR capability and state decoded from the advanced color flags.
struct AdvancedColor {
    hdr_supported: bool,
    hdr_enabled: bool,
}

impl AdvancedColor {
    fn from_flags(flags: u32) -> Self {
        // Wide color enforced means SDR auto color management, not HDR.
        let sdr_wide_color = flags & WIDE_COLOR_ENFORCED != 0;
        let usable = flags & ADVANCED_COLOR_SUPPORTED != 0
            && flags & ADVANCED_COLOR_FORCE_DISABLED == 0
            && !sdr_wide_color;
        AdvancedColor {
            hdr_supported: usable,
            hdr_enabled: usable && flags & ADVANCED_COLOR_ENABLED != 0,
        }
    }
}

/// Reads the real HDR state of the primary display.
pub fn current() -> Result<HdrState> {
    let target = primary_target()?;
    let info = advanced_color(&target)?;
    ensure!(
        info.hdr_supported,
        "primary display does not report HDR support"
    );
    Ok(HdrState::from_enabled(info.hdr_enabled))
}

/// Toggles HDR on the primary display, falling back to Win + Alt + B.
///
/// `fallback_last` is the last known state, flipped when only the fallback is used.
pub fn toggle(fallback_last: &mut HdrState) -> Result<(HdrState, Source)> {
    match toggle_display_config() {
        Ok(state) => {
            *fallback_last = state;
            Ok((state, Source::DisplayConfig))
        }
        Err(reason) => {
            send_win_alt_b().with_context(|| {
                format!("DisplayConfig toggle failed ({reason:#}) and the fallback failed too")
            })?;
            *fallback_last = fallback_last.flipped();
            Ok((*fallback_last, Source::Fallback { reason }))
        }
    }
}

fn toggle_display_config() -> Result<HdrState> {
    let target = primary_target()?;
    let before = advanced_color(&target)?;
    ensure!(
        before.hdr_supported,
        "primary display does not report HDR support"
    );
    set_advanced_color(&target, !before.hdr_enabled)?;
    // The switch already happened; never fall back after this point.
    let enabled = advanced_color(&target)
        .map(|after| after.hdr_enabled)
        .unwrap_or(!before.hdr_enabled);
    Ok(HdrState::from_enabled(enabled))
}

/// Finds the target of the primary display: the active path whose source sits at (0, 0).
fn primary_target() -> Result<Target> {
    let (paths, modes) = query_active_config()?;
    let path = paths
        .iter()
        .find(|path| is_primary_path(path, &modes))
        .or_else(|| paths.first())
        .context("no active display paths")?;
    Ok(Target {
        adapter_id: path.targetInfo.adapterId,
        id: path.targetInfo.id,
    })
}

fn is_primary_path(path: &DISPLAYCONFIG_PATH_INFO, modes: &[DISPLAYCONFIG_MODE_INFO]) -> bool {
    modes.iter().any(|mode| {
        mode.adapterId == path.sourceInfo.adapterId
            && mode.id == path.sourceInfo.id
            && source_position(mode) == Some((0, 0))
    })
}

fn source_position(mode: &DISPLAYCONFIG_MODE_INFO) -> Option<(i32, i32)> {
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
        return None;
    }
    // SAFETY: infoType is SOURCE, so sourceMode is the initialized member of the union.
    let position = unsafe { mode.Anonymous.sourceMode.position };
    Some((position.x, position.y))
}

/// Returns the active display paths and modes, retrying if the topology changes mid-query.
fn query_active_config() -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
    const ATTEMPTS: usize = 3;
    for _ in 0..ATTEMPTS {
        let (path_count, mode_count) = buffer_sizes()?;
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        match query_display_config(&mut paths, &mut modes) {
            Ok((path_count, mode_count)) => {
                paths.truncate(path_count);
                modes.truncate(mode_count);
                return Ok((paths, modes));
            }
            Err(err) if err == ERROR_INSUFFICIENT_BUFFER => continue,
            Err(err) => return Err(win32_error(err, "QueryDisplayConfig")),
        }
    }
    Err(anyhow!(
        "display configuration kept changing during QueryDisplayConfig"
    ))
}

/// Safe wrapper for GetDisplayConfigBufferSizes.
fn buffer_sizes() -> Result<(u32, u32)> {
    let (mut path_count, mut mode_count) = (0u32, 0u32);
    // SAFETY: both out-pointers refer to live, writable local u32 values.
    let err = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    check(err, "GetDisplayConfigBufferSizes")?;
    Ok((path_count, mode_count))
}

/// Safe wrapper for QueryDisplayConfig. Returns how many paths and modes were written.
fn query_display_config(
    paths: &mut [DISPLAYCONFIG_PATH_INFO],
    modes: &mut [DISPLAYCONFIG_MODE_INFO],
) -> std::result::Result<(usize, usize), WIN32_ERROR> {
    let mut path_count = u32::try_from(paths.len()).unwrap_or(u32::MAX);
    let mut mode_count = u32::try_from(modes.len()).unwrap_or(u32::MAX);
    // SAFETY: the counts never exceed the lengths of the writable buffers, and the
    // topology pointer must be null (None) when QDC_ONLY_ACTIVE_PATHS is used.
    let err = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if err == ERROR_SUCCESS {
        Ok((path_count as usize, mode_count as usize))
    } else {
        Err(err)
    }
}

fn header(
    kind: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    size: usize,
    target: &Target,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size as u32,
        adapterId: target.adapter_id,
        id: target.id,
    }
}

/// Safe wrapper for DisplayConfigGetDeviceInfo(GET_ADVANCED_COLOR_INFO).
fn advanced_color(target: &Target) -> Result<AdvancedColor> {
    let mut info = GetAdvancedColorInfo {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            size_of::<GetAdvancedColorInfo>(),
            target,
        ),
        flags: 0,
        _color_encoding: 0,
        _bits_per_color_channel: 0,
    };
    let packet =
        (&mut info as *mut GetAdvancedColorInfo).cast::<DISPLAYCONFIG_DEVICE_INFO_HEADER>();
    // SAFETY: `info` is #[repr(C)] and starts with the header, whose `size` field is the
    // full struct size, so the API only writes within `info`.
    let code = unsafe { DisplayConfigGetDeviceInfo(packet) };
    check_code(code, "DisplayConfigGetDeviceInfo(GET_ADVANCED_COLOR_INFO)")?;
    Ok(AdvancedColor::from_flags(info.flags))
}

/// Safe wrapper for DisplayConfigSetDeviceInfo(SET_ADVANCED_COLOR_STATE).
fn set_advanced_color(target: &Target, enable: bool) -> Result<()> {
    let state = SetAdvancedColorState {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE,
            size_of::<SetAdvancedColorState>(),
            target,
        ),
        flags: if enable { ENABLE_ADVANCED_COLOR } else { 0 },
    };
    let packet =
        (&state as *const SetAdvancedColorState).cast::<DISPLAYCONFIG_DEVICE_INFO_HEADER>();
    // SAFETY: `state` is #[repr(C)], starts with the header, and `size` is its full size,
    // so the API only reads within `state`.
    let code = unsafe { DisplayConfigSetDeviceInfo(packet) };
    check_code(code, "DisplayConfigSetDeviceInfo(SET_ADVANCED_COLOR_STATE)")
}

/// Safe wrapper for SendInput that presses and releases Win + Alt + B.
fn send_win_alt_b() -> Result<()> {
    let keys = [VK_LWIN, VK_MENU, VK_B];
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&key| key_input(key, false))
        .chain(keys.iter().rev().map(|&key| key_input(key, true)))
        .collect();
    // SAFETY: `inputs` is a fully initialized slice of keyboard INPUTs and cbsize is
    // the size of one INPUT, as SendInput requires.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    ensure!(
        sent as usize == inputs.len(),
        "SendInput injected {sent} of {} key events (input may be blocked)",
        inputs.len()
    );
    Ok(())
}

fn key_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn check(err: WIN32_ERROR, what: &str) -> Result<()> {
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(win32_error(err, what))
    }
}

fn check_code(code: i32, what: &str) -> Result<()> {
    check(WIN32_ERROR(code as u32), what)
}

fn win32_error(err: WIN32_ERROR, what: &str) -> anyhow::Error {
    let detail = windows::core::Error::from_hresult(HRESULT::from_win32(err.0));
    anyhow!("{what} failed: {detail}")
}
