; Inno Setup script for Markd — produces a single Markd-Setup-<ver>.exe that
; installs the app and (optionally) makes it the default handler for Markdown
; files. Build it with installer\build-installer.ps1, which also generates the
; icon and passes the version from Cargo.toml via /DMyAppVersion=...

#ifndef MyAppVersion
  #define MyAppVersion "1.0.3"
#endif

#define MyAppName "Markd"
#define MyAppPublisher "Khalid Utsob"
#define MyAppURL "https://github.com/en-arnob/markd"
#define MyAppExeName "markd.exe"
#define MyProgId "Markd.Document"

[Setup]
; A stable AppId keeps upgrades/uninstall tied to the same product across versions.
AppId={{8E9C5A1F-7B4D-49E2-9C3A-2D1F6B0A4E77}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Install per-machine when run as admin, per-user otherwise (no forced UAC).
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Tell the shell associations changed so new icons/defaults apply immediately.
ChangesAssociations=yes
MinVersion=10.0
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
OutputDir=..\dist
OutputBaseFilename=Markd-Setup-{#MyAppVersion}
SetupIconFile=markd.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "associate"; Description: "Set {#MyAppName} as the default app for Markdown files (.md, .markdown, .mdown, .mkd)"; GroupDescription: "File associations:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; --- Application ProgID (how Markd opens a document) ---
Root: HKA; Subkey: "Software\Classes\{#MyProgId}"; ValueType: string; ValueName: ""; ValueData: "Markdown Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#MyProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"
Root: HKA; Subkey: "Software\Classes\{#MyProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\{#MyProgId}\shell\edit\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""

; --- Offer Markd in the "Open with" list for each extension (always) ---
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "{#MyProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "{#MyProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mdown\OpenWithProgids"; ValueType: string; ValueName: "{#MyProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mkd\OpenWithProgids"; ValueType: string; ValueName: "{#MyProgId}"; ValueData: ""; Flags: uninsdeletevalue

; --- Make Markd the default handler (only when the task is selected) ---
Root: HKA; Subkey: "Software\Classes\.md"; ValueType: string; ValueName: ""; ValueData: "{#MyProgId}"; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.markdown"; ValueType: string; ValueName: ""; ValueData: "{#MyProgId}"; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.mdown"; ValueType: string; ValueName: ""; ValueData: "{#MyProgId}"; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.mkd"; ValueType: string; ValueName: ""; ValueData: "{#MyProgId}"; Flags: uninsdeletevalue; Tasks: associate

; --- Default Programs registration (lets users pick Markd in Settings) ---
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Lightweight native Markdown viewer and editor."
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "{#MyProgId}"
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".markdown"; ValueData: "{#MyProgId}"
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mdown"; ValueData: "{#MyProgId}"
Root: HKA; Subkey: "Software\{#MyAppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mkd"; ValueData: "{#MyProgId}"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: "Software\{#MyAppName}\Capabilities"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
