#ifndef Arch
  #error Arch must be supplied by package-windows.ps1
#endif
#if Arch == "arm64"
  #define AllowedArchitectures "arm64"
#else
  #define AllowedArchitectures "x64compatible"
#endif

[Setup]
AppId=com.pigjin.redisdock
AppName=RedisDock
AppVersion={#Version}
AppVerName=RedisDock {#Version}
VersionInfoVersion={#CoreVersion}
AppPublisher=RedisDock contributors
AppPublisherURL=https://github.com/pigjin/rust-rdm
DefaultDirName={localappdata}\Programs\RedisDock
DefaultGroupName=RedisDock
PrivilegesRequired=lowest
ArchitecturesAllowed={#AllowedArchitectures}
ArchitecturesInstallIn64BitMode={#AllowedArchitectures}
OutputDir={#OutputDir}
OutputBaseFilename=RedisDock-{#Version}-windows-{#Arch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile={#SourceDir}\LICENSE
UninstallDisplayIcon={app}\redisdock.exe

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#Binary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\assets\licenses\*.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\RedisDock"; Filename: "{app}\redisdock.exe"
Name: "{autodesktop}\RedisDock"; Filename: "{app}\redisdock.exe"; Tasks: desktopicon
