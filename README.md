# hdr_toggle

A small Windows 11 system tray utility that toggles HDR on the primary display with one click.

- **Left-click** the tray icon to switch HDR on or off.
- **Right-click** for a menu: *Toggle HDR*, *Open Display Settings*, *Start with Windows*, *Quit*.
- The icon shows the current state: **green** = HDR on, **gray** = HDR off.
- Hovering shows a tooltip: "HDR on" / "HDR off".

No external image files, no background services. One ~650 KB executable, with an optional
per-user installer.

Further reading:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it works internally (modules, HDR API, fallback, unsafe code).
- [docs/TESTING.md](docs/TESTING.md): a manual test checklist.

---

## Contents

1. [Installing](#installing)
2. [Requirements](#requirements)
3. [Building](#building)
4. [Running](#running)
5. [Using the tray icon](#using-the-tray-icon)
6. [Start with Windows](#start-with-windows)
7. [How HDR is switched](#how-hdr-is-switched)
8. [Logs](#logs)
9. [Troubleshooting](#troubleshooting)
10. [Known limitations](#known-limitations)
11. [Uninstalling](#uninstalling)
12. [Releasing](#releasing)
13. [Project layout](#project-layout)

---

## Installing

1. Download `hdr_toggle-x.y.z-setup.exe` from the
   [Releases page](https://github.com/AbdulrahmanKikia/hdr-toggle/releases/latest).
2. Run it. No admin rights are needed: it installs for the current user only, into
   `%LOCALAPPDATA%\Programs\hdr_toggle`.
3. Choose the options you want:
   - **Start HDR Toggle when I sign in to Windows** (ticked by default). This is the same
     setting as *Start with Windows* in the tray menu.
   - **Create a desktop shortcut** (unticked by default).
4. Leave *Launch HDR Toggle* ticked on the last page to start it straight away.

The installer adds a Start Menu shortcut and an entry under *Settings > Apps > Installed apps*.
To upgrade, run the setup for the newer version. It closes the running app, replaces the exe,
and keeps your choices.

> The installer isn't code-signed, so Windows SmartScreen may show *"Windows protected your
> PC"*. Click **More info > Run anyway**.

To build from source instead, read on.

---

## Requirements

**To run**

- Windows 11 (Windows 10 with HDR support should also work) on x64.
- An HDR-capable display with HDR available in *Settings > System > Display*.
  Without one, the app still runs but can only send the Win + Alt + B shortcut (see
  [How HDR is switched](#how-hdr-is-switched)).

**To build**

- [Rust](https://rustup.rs/) stable with the MSVC toolchain:

  ```powershell
  winget install Rustlang.Rustup
  rustup default stable
  ```

- Visual Studio 2022 Build Tools with the *Desktop development with C++* (VCTools) workload,
  which provides the MSVC linker:

  ```powershell
  winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  ```

Tested with Rust 1.99.0 stable. Crate versions are pinned exactly in `Cargo.toml`, and
`Cargo.lock` is committed.

## Building

From the repository root:

```powershell
git clone https://github.com/AbdulrahmanKikia/hdr-toggle.git
cd hdr-toggle
cargo build --release
```

The executable is written to:

```
target\release\hdr_toggle.exe
```

Optional checks:

```powershell
cargo clippy --all-targets   # lints (currently zero warnings)
cargo fmt --check            # formatting
```

> If `CARGO_TARGET_DIR` is set in your environment, Cargo writes the build there instead
> of `target\`.

## Running

```powershell
.\target\release\hdr_toggle.exe
```

- The release build has **no console window**; only the tray icon appears.
- A debug build (`cargo run`) opens a console window. This is useful only for seeing panics,
  since errors go to the [log file](#logs).
- **Only one instance runs at a time.** Launching it again while it is running exits
  immediately and silently.

**Finding the icon:** Windows 11 hides new tray icons in the overflow menu (the **^** arrow
next to the clock). To keep it visible, drag it onto the taskbar, or enable it under
*Settings > Personalization > Taskbar > Other system tray icons*.

### Installing to a permanent location (recommended)

`target\release` is a build folder. `cargo clean` deletes it, and a rebuild can't overwrite
the exe while the app is running. For everyday use, use the [installer](#installing), or
copy the exe somewhere stable:

```powershell
$dest = "$env:LOCALAPPDATA\Programs\hdr_toggle"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item .\target\release\hdr_toggle.exe $dest
& "$dest\hdr_toggle.exe"
```

Then turn on *Start with Windows* from the copied exe so autostart points at it.

## Using the tray icon

| Action | Result |
| --- | --- |
| Left-click | Toggles HDR, then updates the icon and tooltip. |
| Hover | Re-reads the real HDR state, so the icon stays correct if HDR was changed elsewhere (for example in Settings or with Win + Alt + B). |
| Right-click > **Toggle HDR** | Same as left-click. |
| Right-click > **Open Display Settings** | Opens *Settings > System > Display* (`ms-settings:display`). |
| Right-click > **Start with Windows** | Checkbox. Adds or removes the app from startup. |
| Right-click > **Quit** | Removes the tray icon and exits. |

**Icon and tooltip states**

| Icon | Tooltip | Meaning |
| --- | --- | --- |
| Green | `HDR on` | HDR is on. The state was read from Windows. |
| Gray | `HDR off` | HDR is off. The state was read from Windows. |
| Green | `HDR on (assumed - fallback, state not read)` | The app is in fallback mode. It sent Win + Alt + B and *assumes* HDR is now on. |
| Gray | `HDR off (assumed - fallback, state not read)` | Fallback mode, assumed off. |

## Start with Windows

Ticking *Start with Windows* writes this registry value:

```
Key:   HKCU\Software\Microsoft\Windows\CurrentVersion\Run
Name:  HDRToggle
Value: "C:\path\to\hdr_toggle.exe"     (the exe that is currently running, quoted)
```

Unticking deletes the value. The setting is per-user and needs no admin rights.

- On launch, the checkbox is ticked only if the value exists **and** points at the exe that
  is currently running. If you move the exe, the box shows unticked. Tick it again to update
  the path.
- If writing to the registry fails, the error is logged and the checkbox reverts to match
  the registry.
- You can also manage the entry in *Settings > Apps > Startup* or Task Manager's
  *Startup apps* tab, where it is listed as `HDRToggle`.

Check it from PowerShell:

```powershell
Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run -Name HDRToggle
```

## How HDR is switched

1. **Preferred: the DisplayConfig API.** The app finds the primary display (the one whose
   desktop starts at coordinates 0,0), reads its *advanced color* state with
   `DisplayConfigGetDeviceInfo`, and flips it with `DisplayConfigSetDeviceInfo`. It then reads
   the state back, so the icon reflects what Windows actually did. These are documented
   Win32 APIs, the same mechanism behind the *Use HDR* switch in Settings.

2. **Fallback: Win + Alt + B.** If any DisplayConfig call fails, or the display does not
   report HDR support, the app simulates the Windows HDR keyboard shortcut with `SendInput`.
   In this mode Windows can't be asked for the state, so the app tracks it by flipping its
   last known value on each toggle. The tooltip says "(assumed ...)" to make this clear.

A display running SDR *Auto Color Management* reports "advanced color" too. The app does not
count that as HDR, so it uses the fallback for such displays.

Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#hdr-detection-and-switching).

## Logs

Errors are appended to:

```
%LOCALAPPDATA%\hdr_toggle\log.txt
```

Each line has a UTC timestamp, for example:

```
[2026-10-14 05:15:43 UTC] used Win+Alt+B fallback: primary display does not report HDR support
```

**If the file doesn't exist, no errors have occurred.** Common entries:

| Message (start) | Meaning |
| --- | --- |
| `cannot read HDR state; tracking it from toggles` | At startup, the HDR state couldn't be read. The app starts in fallback mode, assuming HDR is off. |
| `used Win+Alt+B fallback: ...` | A toggle used the keyboard shortcut. The rest of the line says why the API was not used. |
| `DisplayConfig toggle failed (...) and the fallback failed too` | Neither method worked. Nothing changed. |
| `failed to ... autostart registry value` | Reading or writing the Run key failed. |
| `failed to create tray icon` | The tray icon couldn't be created. The app exits. |

The log is never rotated. Delete it whenever you like.

Show the most recent entries:

```powershell
Get-Content "$env:LOCALAPPDATA\hdr_toggle\log.txt" -Tail 20
```

## Troubleshooting

**No tray icon appears.**
Check the overflow area (**^**). If the app isn't running at all, another instance may
already be running hidden: `Get-Process hdr_toggle`. Check the log for `failed to create tray icon`.

**Clicking does nothing / HDR doesn't change.**

- Make sure *Use HDR* is available for the primary display in *Settings > System > Display*.
- Check the log. If it shows the fallback was used, try pressing **Win + Alt + B** yourself.
  That shortcut is handled by Windows (it may depend on Xbox Game Bar being enabled). If the
  manual shortcut does nothing, the fallback can't work either.
- `SendInput` can't inject keys into elevated (admin) windows from a non-elevated app. If an
  admin window has focus, the fallback can fail.

**The icon shows the wrong state.**
Hover over it. In normal mode this re-reads the real state. In fallback mode the state is
only assumed and can drift if HDR is changed elsewhere. Quit and restart the app to reset it.

**The app doesn't start at login.**
Check that the Run value exists and points at an exe that still exists (see
[Start with Windows](#start-with-windows)). It may have pointed at `target\release` and been
removed by `cargo clean`. Also make sure it isn't disabled in *Settings > Apps > Startup*.

**`cargo build` fails with `link.exe not found`.**
Install the VS 2022 Build Tools with the VCTools workload (see [Requirements](#requirements)).

**Rebuild fails with "Access is denied" on `hdr_toggle.exe`.**
The app is running and locks its exe. Quit it from the tray first, or run
`Stop-Process -Name hdr_toggle`.

## Known limitations

- **Primary display only.** Other displays are not affected. "Primary" means the active
  display whose desktop origin is at (0, 0). If none matches, the first active display is used.
- **Fallback state is assumed.** In fallback mode the app starts by assuming HDR is off, and
  it flips that guess on each toggle even if the shortcut had no effect (for example on a
  non-HDR display).
- **Older HDR API.** Windows 11 24H2 added newer HDR calls
  (`DISPLAYCONFIG_DEVICE_INFO_SET_HDR_STATE`). This app uses the older documented
  advanced-color calls, which still work on current Windows 11.
- **Tooltip only.** There is no toast or balloon notification on toggle.
- **The autostart path is fixed.** If the exe moves, re-tick *Start with Windows*.
- **The log grows forever.** It is never rotated, though in practice it only gets error lines.

## Uninstalling

**If you used the installer:** uninstall *HDR Toggle* from *Settings > Apps > Installed apps*.
This closes the app and removes the exe, the shortcuts, the `HDRToggle` Run value, and the
log folder.

**If you built or copied the exe yourself:**

1. Untick *Start with Windows* (or delete the `HDRToggle` value under the Run key).
2. Choose *Quit* from the tray menu.
3. Delete the exe (and its folder, if you copied it to `%LOCALAPPDATA%\Programs\hdr_toggle`).
4. Optionally delete the logs: `Remove-Item -Recurse "$env:LOCALAPPDATA\hdr_toggle"`.

## Releasing

The installer is built with [Inno Setup 6](https://jrsoftware.org/isinfo.php) from
`installer\hdr_toggle.iss`.

**Build it locally:**

```powershell
winget install JRSoftware.InnoSetup    # once
.\installer\build.ps1
```

This runs `cargo build --release --locked` and writes
`installer\Output\hdr_toggle-<version>-setup.exe`, using the version from `Cargo.toml`.

**Publish a release:**

1. Bump `version` in `Cargo.toml` (and run `cargo build` so `Cargo.lock` picks it up), then commit.
2. Tag and push:

   ```powershell
   git tag v0.2.0
   git push origin v0.2.0
   ```

The [Release workflow](.github/workflows/release.yml) builds the installer on GitHub Actions
and attaches it to a new GitHub Release. It fails if the tag doesn't match the `Cargo.toml`
version. You can also run the workflow manually from the *Actions* tab to get the installer
as a build artifact without publishing a release.

## Project layout

```
.
├── Cargo.toml          # dependencies (exact pins) and windows-rs features
├── Cargo.lock
├── README.md           # this file
├── .github/workflows/
│   └── release.yml     # builds the installer and publishes GitHub Releases on v* tags
├── docs/
│   ├── ARCHITECTURE.md # internals
│   └── TESTING.md      # manual test checklist
├── installer/
│   ├── hdr_toggle.iss  # Inno Setup script (per-user installer)
│   └── build.ps1       # builds the exe and the installer
└── src/
    ├── main.rs         # event loop, tray + menu, single instance, logging
    ├── hdr.rs          # HDR query/set (DisplayConfig) + Win+Alt+B fallback
    ├── autostart.rs    # HKCU Run registry value
    └── icon.rs         # 32x32 RGBA circle icon (green/gray)
```

Dependencies: [`windows`](https://crates.io/crates/windows) 0.62.2,
[`tray-icon`](https://crates.io/crates/tray-icon) 0.26.1,
[`tao`](https://crates.io/crates/tao) 0.37.1,
[`winreg`](https://crates.io/crates/winreg) 0.56.0,
[`anyhow`](https://crates.io/crates/anyhow) 1.0.104.
