//! "Start with Windows" support via the per-user Run registry key.

use std::io::ErrorKind;

use anyhow::{Context, Result};
use winreg::enums::{KEY_QUERY_VALUE, KEY_SET_VALUE};
use winreg::{RegKey, HKCU};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "HDRToggle";

/// Returns true if the Run value exists and points at the current executable.
pub fn is_enabled() -> Result<bool> {
    let key = open_run_key(KEY_QUERY_VALUE)?;
    match key.get_value::<String, _>(VALUE_NAME) {
        Ok(value) => Ok(value.eq_ignore_ascii_case(&exe_command()?)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("failed to read autostart registry value"),
    }
}

/// Adds or removes the Run value.
pub fn set(enabled: bool) -> Result<()> {
    if enabled {
        enable()
    } else {
        disable()
    }
}

fn enable() -> Result<()> {
    open_run_key(KEY_SET_VALUE)?
        .set_value(VALUE_NAME, &exe_command()?)
        .context("failed to write autostart registry value")
}

fn disable() -> Result<()> {
    match open_run_key(KEY_SET_VALUE)?.delete_value(VALUE_NAME) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("failed to delete autostart registry value"),
    }
}

fn open_run_key(access: u32) -> Result<RegKey> {
    HKCU.open_subkey_with_flags(RUN_KEY, access)
        .context("failed to open HKCU Run registry key")
}

/// The quoted path of the running executable, as stored in the Run value.
fn exe_command() -> Result<String> {
    let exe = std::env::current_exe().context("failed to get current exe path")?;
    Ok(format!("\"{}\"", exe.display()))
}
