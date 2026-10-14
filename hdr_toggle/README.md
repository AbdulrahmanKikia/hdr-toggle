# hdr_toggle

A small Windows 11 system tray utility that toggles HDR on the primary display.

- **Left-click** the tray icon to toggle HDR.
- **Right-click** for the menu: *Toggle HDR*, *Open Display Settings*, *Start with Windows*, *Quit*.
- The icon is green when HDR is on and gray when it is off. The tooltip says "HDR on" / "HDR off".

HDR is read and set with the documented DisplayConfig API
(`DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO` / `DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE`).
If that fails or the display does not report HDR support, the app sends the Win + Alt + B
shortcut instead. In that case the state cannot be read, so the app tracks it from its own
toggles and the tooltip adds "(assumed - fallback, state not read)".

## Prerequisites

- Windows 10/11 (x64)
- Rust stable with the MSVC toolchain (`rustup default stable`)
- Visual Studio 2022 Build Tools with the "Desktop development with C++" (VCTools) workload

## Build

```powershell
cd hdr_toggle
cargo build --release
```

## Run

```powershell
.\target\release\hdr_toggle.exe
```

The release build has no console window. Only one instance runs at a time; starting it
again exits immediately.

## Start with Windows

The *Start with Windows* checkbox adds or removes the `HDRToggle` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, pointing at the current exe path.
If you move the exe, toggle the checkbox again to update the path.

## Logs

Errors are appended to `%LOCALAPPDATA%\hdr_toggle\log.txt`.
