# Testing

`hdr_toggle` has no automated tests. Its behaviour depends on real display hardware,
the Windows shell, and the registry, so it is tested by hand with the checklist below.

## Before you start

```powershell
cd hdr-toggle                  # the repository root
cargo build --release
cargo clippy --all-targets      # expect: no warnings
Stop-Process -Name hdr_toggle -ErrorAction SilentlyContinue   # make sure no copy is running
$log = "$env:LOCALAPPDATA\hdr_toggle\log.txt"
Remove-Item $log -ErrorAction SilentlyContinue                # start with an empty log
```

Have *Settings > System > Display* open next to you to confirm the real HDR state.

## Checklist

### 1. Launch

| Step | Expected |
| --- | --- |
| Run `.\target\release\hdr_toggle.exe` | No console window. A tray icon appears (check the **^** overflow). |
| Look at the icon | Green if *Use HDR* is on in Settings, gray if off. |
| Hover over the icon | Tooltip `HDR on` / `HDR off`, without "(assumed ...)". |
| `Test-Path $log` | `False`: startup produced no errors. |

If the tooltip says "(assumed ...)", the app couldn't read HDR state. The log explains why.

### 2. Toggle

| Step | Expected |
| --- | --- |
| Left-click the icon | Screen flickers briefly; HDR switches. Icon colour and tooltip change. |
| Check Settings | *Use HDR* matches the icon. |
| Left-click again | Switches back. |
| Right-click > **Toggle HDR** | Same behaviour as left-click. |
| Right-click the icon | Only the menu opens; HDR does **not** toggle. |

### 3. External changes

| Step | Expected |
| --- | --- |
| Flip *Use HDR* in Settings | Icon may be stale at first. |
| Hover over the tray icon | Icon and tooltip update to the new state. |
| Press **Win + Alt + B**, then hover | Same: the icon follows the real state. |

### 4. Display Settings

| Step | Expected |
| --- | --- |
| Right-click > **Open Display Settings** | Settings opens on *System > Display*. |

### 5. Start with Windows

```powershell
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
```

| Step | Expected |
| --- | --- |
| Right-click > tick **Start with Windows** | Checkbox ticked. |
| `(Get-ItemProperty $run).HDRToggle` | Quoted full path to the running `hdr_toggle.exe`. |
| Quit and relaunch the app | Checkbox still ticked. |
| Untick **Start with Windows** | Checkbox unticked. |
| `(Get-ItemProperty $run).HDRToggle` | Empty (value removed). |
| Tick it again, sign out, sign back in | App starts automatically; icon appears. |

### 6. Single instance

| Step | Expected |
| --- | --- |
| With the app running, launch the exe again | Nothing new appears; still one icon. |
| `(Get-Process hdr_toggle).Count` | `1` |

Scripted version:

```powershell
$exe = '.\target\release\hdr_toggle.exe'
$p = Start-Process $exe -PassThru; Start-Sleep 2
"second instance exited: $($p.HasExited) (exit code $($p.ExitCode))"   # expect True (0)
```

### 7. Quit

| Step | Expected |
| --- | --- |
| Right-click > **Quit** | Icon disappears at once. |
| `Get-Process hdr_toggle` | Not found. |

### 8. Log

| Step | Expected |
| --- | --- |
| `Get-Content $log` | Missing or empty after a normal session. Any lines start with a UTC timestamp `[YYYY-MM-DD HH:MM:SS UTC]`. |

## Testing the installer

Build it with `.\installer\build.ps1` (needs Inno Setup 6), then run
`installer\Output\hdr_toggle-<version>-setup.exe` as a **non-admin** user.

```powershell
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$app = "$env:LOCALAPPDATA\Programs\hdr_toggle"
```

### Fresh install

| Step | Expected |
| --- | --- |
| Run the setup | No UAC prompt. The wizard offers *Start ... when I sign in* (ticked) and *Create a desktop shortcut* (unticked). |
| Finish with *Launch HDR Toggle* ticked | The tray icon appears. |
| `Test-Path "$app\hdr_toggle.exe"` | `True` |
| Start Menu | Has an *HDR Toggle* shortcut that launches the app. |
| `(Get-ItemProperty $run).HDRToggle` | `"C:\Users\<you>\AppData\Local\Programs\hdr_toggle\hdr_toggle.exe"` (quoted). |
| Right-click the tray icon | *Start with Windows* is **ticked**. |
| *Settings > Apps > Installed apps* | Lists *HDR Toggle* with the right version. |

### Autostart task off

| Step | Expected |
| --- | --- |
| Re-run the setup and untick *Start ... when I sign in* | Setup completes. |
| `(Get-ItemProperty $run).HDRToggle` | Empty (value removed). |
| Right-click the tray icon | *Start with Windows* is unticked. |

### Upgrade while running

| Step | Expected |
| --- | --- |
| With the app running, run the setup again (or a newer version) | No "files in use" error. The old instance is closed and the exe is replaced. |
| `(Get-Process hdr_toggle).Count` after finishing with *Launch* ticked | `1` |

### Uninstall

| Step | Expected |
| --- | --- |
| Tick *Start with Windows* from the tray, then uninstall from *Installed apps* | The app closes and the tray icon disappears (hover if a stale icon remains). |
| `Test-Path $app` | `False` |
| `(Get-ItemProperty $run).HDRToggle` | Empty (value removed even though the tray menu set it). |
| Start Menu / desktop | No *HDR Toggle* shortcuts. |
| `Test-Path "$env:LOCALAPPDATA\hdr_toggle"` | `False` (log folder removed). |

## Testing the fallback

On a working HDR display the API path is always used, so the fallback has to be tested
deliberately.

**Check that the shortcut works on this PC:** press **Win + Alt + B** yourself. If HDR does
not toggle, the fallback cannot work here either (the shortcut may depend on Xbox Game Bar
being enabled).

**Force the app into fallback mode** (temporary code change, do not commit). In `src/hdr.rs`,
make `toggle_display_config` fail straight away:

```rust
fn toggle_display_config() -> Result<HdrState> {
    anyhow::bail!("forced fallback for testing");
    // ...original body unchanged below...
}
```

`current()` will still succeed at startup, so the app starts in normal mode. Rebuild, run it,
and then:

| Step | Expected |
| --- | --- |
| Left-click | HDR toggles through the simulated shortcut. Tooltip ends with `(assumed - fallback, state not read)`. |
| `Get-Content $log` | A line `used Win+Alt+B fallback: forced fallback for testing`. |
| Hover | Nothing changes. The hover refresh is skipped in assumed mode. |

Revert the change and rebuild afterwards (`git checkout src/hdr.rs`).

To test starting **in** fallback mode, make `current()` return an error the same way. The
icon should start gray with "(assumed ...)" in the tooltip, and the log should contain
`cannot read HDR state; tracking it from toggles`.

## Environment edge cases worth trying

- **No HDR display attached / HDR unavailable:** the app should start in assumed mode and
  log why. Clicking sends Win + Alt + B (which may do nothing). The app must not crash.
- **Multiple monitors:** only the primary display (*Make this my main display*) should toggle.
  Change which monitor is primary and toggle again.
- **Elevated window in focus during a fallback toggle:** `SendInput` may be blocked. Expect a
  log line saying fewer than 6 key events were injected.
- **Exe path with spaces:** the default project path already contains a space
  (`HDR Toggle`), so autostart covers this.

## Results so far

Tested during development on Windows 11 with an HDR-capable primary display:

| Area | Result |
| --- | --- |
| `cargo build --release`, `cargo clippy --all-targets` | Pass, no warnings |
| Launch, startup HDR read via DisplayConfig | Pass (no log written) |
| Single instance (second launch exits with code 0) | Pass |
| Autostart value added from PowerShell in the app's format | Done; the checkbox and login start are not yet verified |
| Toggle by click, menu items, fallback path | Not yet verified by hand |
