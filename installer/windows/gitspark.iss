; GitSpark — Inno Setup installer script
; Produces a per-user installer that places the app in %LOCALAPPDATA%\GitSpark.
; Per-user (no admin) is what lets the app's own auto-updater replace
; gitspark.exe in place without elevation.
;
; Build from the repo root:
;   iscc /DAppVersion=0.8.3 /DSourceDir=dist\gitspark-v0.8.3-x86_64-pc-windows-msvc installer\windows\gitspark.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#ifndef SourceDir
  #define SourceDir "..\..\dist"
#endif

[Setup]
AppId={{22D9E69C-A16B-4AA4-8B6E-280A47F25827}
AppName=GitSpark
AppVersion={#AppVersion}
AppVerName=GitSpark {#AppVersion}
AppPublisher=GitSpark
AppPublisherURL=https://github.com/JacobSamro/GitSpark
AppSupportURL=https://github.com/JacobSamro/GitSpark/issues
DefaultDirName={localappdata}\GitSpark
DisableDirPage=yes
DefaultGroupName=GitSpark
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=gitspark-setup
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=lowest
SetupIconFile=..\..\assets\gitspark.ico
UninstallDisplayIcon={app}\gitspark.exe
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
Source: "{#SourceDir}\gitspark.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\GitSpark"; Filename: "{app}\gitspark.exe"
Name: "{group}\Uninstall GitSpark"; Filename: "{uninstallexe}"
Name: "{autodesktop}\GitSpark"; Filename: "{app}\gitspark.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\gitspark.exe"; Description: "Launch GitSpark"; Flags: nowait postinstall skipifsilent
