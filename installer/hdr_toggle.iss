; Inno Setup 6 script for hdr_toggle.
;
; Build with installer\build.ps1, or directly:
;   iscc /DAppVersion=0.1.0 installer\hdr_toggle.iss
;
; Per-user install: no admin rights, everything lives under the current user's profile
; and HKCU, matching what the app itself touches.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#ifndef ExePath
  #define ExePath "..\target\release\hdr_toggle.exe"
#endif

#define AppName "HDR Toggle"
#define AppExeName "hdr_toggle.exe"
#define AppUrl "https://github.com/AbdulrahmanKikia/hdr-toggle"
; Must match RUN_KEY / VALUE_NAME in src/autostart.rs.
#define RunKey "Software\Microsoft\Windows\CurrentVersion\Run"
#define RunValue "HDRToggle"

[Setup]
; Never change AppId: it is how upgrades and the uninstaller find the existing install.
AppId={{7C3E9A52-4B1D-4F8E-9A63-2D5B8E1F0C47}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Abdulrahman Kikia
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
DefaultDirName={localappdata}\Programs\hdr_toggle
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=Output
OutputBaseFilename=hdr_toggle-{#AppVersion}-setup
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startup"; Description: "Start {#AppName} when I sign in to Windows"; GroupDescription: "Startup:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; The quoted-path format must match exe_command() in src/autostart.rs, otherwise the
; tray menu's "Start with Windows" checkbox shows as unticked.
Root: HKCU; Subkey: "{#RunKey}"; ValueType: string; ValueName: "{#RunValue}"; ValueData: """{app}\{#AppExeName}"""; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#AppExeName}"; Flags: runhidden waituntilterminated; RunOnceId: "StopApp"

[UninstallDelete]
; Log folder written by the app (see README "Logs").
Type: filesandordirs; Name: "{localappdata}\hdr_toggle"

[Code]
procedure StopRunningApp();
var
  ResultCode: Integer;
begin
  // The app is tray-only, so Restart Manager can't ask it to close; it would keep the
  // exe locked and the upgrade would fail to overwrite it.
  if Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#AppExeName}', '', SW_HIDE,
          ewWaitUntilTerminated, ResultCode) and (ResultCode = 0) then
    Sleep(500);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningApp();
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // Keep the Run value in sync with the task when an upgrade unticks it.
  if (CurStep = ssPostInstall) and not WizardIsTaskSelected('startup') then
    RegDeleteValue(HKEY_CURRENT_USER, '{#RunKey}', '{#RunValue}');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // The tray menu can create the Run value after install, so remove it unconditionally.
  if CurUninstallStep = usUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, '{#RunKey}', '{#RunValue}');
end;
