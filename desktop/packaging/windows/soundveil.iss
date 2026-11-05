; Builds the Windows installer. It installs for the current person, so it asks for no admin rights.
; Run with ISCC /DAppVersion=0.1.1 /DBinaryDir=<folder holding the program> soundveil.iss

#define AppName "Soundveil"

[Setup]
AppId={{6F7C2B1E-8E3A-4C55-9F6D-2A1B7D3E4C10}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Soundveil
AppPublisherURL=https://github.com/JingdaKang/soundveil
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=soundveil-{#AppVersion}-windows-x64-setup
OutputDir=output
SetupIconFile=soundveil.ico
UninstallDisplayIcon={app}\soundveil.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "Add a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#BinaryDir}\soundveil.exe"; DestDir: "{app}"; Flags: ignoreversion
; GPU recognition needs this. Graphics drivers normally bring their own, but not every computer has one.
Source: "{#BinaryDir}\vulkan-1.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\soundveil.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\soundveil.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\soundveil.exe"; Description: "Open Soundveil"; Flags: nowait postinstall skipifsilent
