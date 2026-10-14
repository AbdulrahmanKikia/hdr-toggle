#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod hdr;
mod icon;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

use hdr::{HdrState, Source};

enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

fn main() {
    if let Err(err) = run() {
        log_error(&err);
    }
}

fn run() -> Result<()> {
    let Some(instance) = SingleInstance::acquire()? else {
        return Ok(());
    };
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    forward_events(event_loop.create_proxy());
    let mut app = App::new();

    event_loop.run(move |event, _, control_flow| {
        let _ = &instance;
        *control_flow = ControlFlow::Wait;
        let keep_running = match event {
            Event::NewEvents(StartCause::Init) => app.init_tray(),
            Event::UserEvent(UserEvent::Tray(event)) => app.on_tray_event(event),
            Event::UserEvent(UserEvent::Menu(event)) => app.on_menu_event(&event),
            _ => true,
        };
        if !keep_running {
            app.tray = None;
            *control_flow = ControlFlow::Exit;
        }
    })
}

/// Routes tray and menu callbacks (which may fire on other threads) into the event loop.
fn forward_events(proxy: EventLoopProxy<UserEvent>) {
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = tray_proxy.send_event(UserEvent::Tray(event));
    }));
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));
}

struct MenuItems {
    toggle: MenuItem,
    settings: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

impl MenuItems {
    fn new() -> Self {
        let autostart_on = autostart::is_enabled().unwrap_or_else(|err| {
            log_error(&err);
            false
        });
        MenuItems {
            toggle: MenuItem::new("Toggle HDR", true, None),
            settings: MenuItem::new("Open Display Settings", true, None),
            autostart: CheckMenuItem::new("Start with Windows", true, autostart_on, None),
            quit: MenuItem::new("Quit", true, None),
        }
    }

    fn build_menu(&self) -> Result<Menu> {
        let menu = Menu::new();
        menu.append_items(&[
            &self.toggle,
            &self.settings,
            &self.autostart,
            &PredefinedMenuItem::separator(),
            &self.quit,
        ])
        .context("failed to build tray menu")?;
        Ok(menu)
    }
}

struct App {
    tray: Option<TrayIcon>,
    items: MenuItems,
    state: HdrState,
    /// True when `state` is tracked from fallback toggles rather than read from the display.
    assumed: bool,
}

impl App {
    fn new() -> Self {
        let (state, assumed) = match hdr::current() {
            Ok(state) => (state, false),
            Err(err) => {
                log_error(&err.context("cannot read HDR state; tracking it from toggles"));
                (HdrState::Off, true)
            }
        };
        App {
            tray: None,
            items: MenuItems::new(),
            state,
            assumed,
        }
    }

    /// Creates the tray icon. Returns false if that fails, since the app has no other UI.
    fn init_tray(&mut self) -> bool {
        match self.build_tray() {
            Ok(tray) => {
                self.tray = Some(tray);
                true
            }
            Err(err) => {
                log_error(&err);
                false
            }
        }
    }

    fn build_tray(&self) -> Result<TrayIcon> {
        TrayIconBuilder::new()
            .with_menu(Box::new(self.items.build_menu()?))
            .with_menu_on_left_click(false)
            .with_icon(icon::make_icon(self.state.is_on())?)
            .with_tooltip(self.tooltip())
            .build()
            .context("failed to create tray icon")
    }

    fn on_tray_event(&mut self, event: TrayIconEvent) -> bool {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => self.toggle(),
            TrayIconEvent::Enter { .. } if !self.assumed => self.refresh(),
            _ => {}
        }
        true
    }

    /// Handles a menu click. Returns false when the app should quit.
    fn on_menu_event(&mut self, event: &MenuEvent) -> bool {
        let id = &event.id;
        if id == self.items.toggle.id() {
            self.toggle();
        } else if id == self.items.settings.id() {
            if let Err(err) = open_display_settings() {
                log_error(&err);
            }
        } else if id == self.items.autostart.id() {
            self.apply_autostart();
        } else if id == self.items.quit.id() {
            return false;
        }
        true
    }

    fn toggle(&mut self) {
        match hdr::toggle(&mut self.state) {
            Ok((_, Source::DisplayConfig)) => self.assumed = false,
            Ok((_, Source::Fallback { reason })) => {
                log_error(&reason.context("used Win+Alt+B fallback"));
                self.assumed = true;
            }
            Err(err) => log_error(&err),
        }
        self.update_tray();
    }

    /// Re-reads the real HDR state, in case it was changed outside this app.
    fn refresh(&mut self) {
        if let Ok(state) = hdr::current() {
            if state != self.state {
                self.state = state;
                self.update_tray();
            }
        }
    }

    /// Writes the checkbox state to the registry, then re-syncs the checkbox from it.
    fn apply_autostart(&self) {
        if let Err(err) = autostart::set(self.items.autostart.is_checked()) {
            log_error(&err);
        }
        match autostart::is_enabled() {
            Ok(enabled) => self.items.autostart.set_checked(enabled),
            Err(err) => log_error(&err),
        }
    }

    fn update_tray(&self) {
        let Some(tray) = &self.tray else { return };
        let icon_result = icon::make_icon(self.state.is_on())
            .and_then(|icon| tray.set_icon(Some(icon)).context("failed to set tray icon"));
        if let Err(err) = icon_result {
            log_error(&err);
        }
        if let Err(err) = tray.set_tooltip(Some(self.tooltip())) {
            log_error(&anyhow::Error::new(err).context("failed to set tray tooltip"));
        }
    }

    fn tooltip(&self) -> String {
        let text = if self.state.is_on() {
            "HDR on"
        } else {
            "HDR off"
        };
        if self.assumed {
            format!("{text} (assumed - fallback, state not read)")
        } else {
            text.to_string()
        }
    }
}

fn open_display_settings() -> Result<()> {
    // explorer.exe hands the URI to the Settings app and exits; its exit code is not meaningful.
    Command::new("explorer.exe")
        .arg("ms-settings:display")
        .status()
        .context("failed to open Display Settings")?;
    Ok(())
}

/// Owns the named mutex that prevents a second instance from running.
struct SingleInstance(HANDLE);

impl SingleInstance {
    /// Returns None if another instance already holds the mutex.
    fn acquire() -> Result<Option<Self>> {
        let (handle, already_exists) = create_named_mutex()?;
        let guard = SingleInstance(handle);
        Ok((!already_exists).then_some(guard))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        close_handle(self.0);
    }
}

/// Safe wrapper for CreateMutexW. Returns the handle and whether the mutex already existed.
fn create_named_mutex() -> Result<(HANDLE, bool)> {
    // SAFETY: the name is a static NUL-terminated wide string, no security attributes are
    // passed, and GetLastError is read right after CreateMutexW on the same thread.
    unsafe {
        let handle = CreateMutexW(None, false, w!("Local\\hdr_toggle_single_instance"))
            .context("CreateMutexW failed")?;
        Ok((handle, GetLastError() == ERROR_ALREADY_EXISTS))
    }
}

/// Safe wrapper for CloseHandle.
fn close_handle(handle: HANDLE) {
    // SAFETY: the handle came from CreateMutexW and is owned and closed only by SingleInstance.
    let _ = unsafe { CloseHandle(handle) };
}

/// Appends an error to %LOCALAPPDATA%\hdr_toggle\log.txt. Logging failures are ignored.
fn log_error(err: &anyhow::Error) {
    let _ = append_log(&format!("{err:#}"));
}

fn append_log(message: &str) -> io::Result<()> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("log.txt"))?;
    writeln!(file, "[{}] {message}", utc_timestamp())
}

fn log_dir() -> io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join("hdr_toggle"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))
}

fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Converts days since 1970-01-01 to a (year, month, day) civil date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
