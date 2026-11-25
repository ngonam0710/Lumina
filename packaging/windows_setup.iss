#ifndef ReleaseDir
#define ReleaseDir "..\target\release"
#endif
[Setup]
AppId=LuminaDesktop
AppName=Lumina
AppVersion=0.3.0
AppPublisher=Lumina
DefaultDirName={localappdata}\Programs\LuminaDesktop
DefaultGroupName=Lumina
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
SetupArchitecture=x64
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\assets\lumina.ico
CreateUninstallRegKey=RegisterUninstaller
UninstallDisplayIcon={app}\lumina.exe
OutputDir=..\target
OutputBaseFilename=Lumina-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
AppMutex=Local\LuminaRS_SingleInstance
RestartApplications=no
[Languages]
Name: "vi"; MessagesFile: "Vietnamese.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"
[Files]
Source: "{#ReleaseDir}\lumina.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\lumina-guard.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\fonts\Inter-*.ttf"; DestDir: "{app}\fonts"; Flags: ignoreversion
Source: "..\assets\fonts\Inter-LICENSE.txt"; DestDir: "{app}\fonts"; Flags: ignoreversion
[Icons]
Name: "{group}\Lumina"; Filename: "{app}\lumina.exe"; Parameters: "--open-settings"; WorkingDir: "{app}"
[Run]
Filename: "{app}\lumina.exe"; Parameters: "--open-settings"; Flags: postinstall nowait skipifsilent

[Code]
function RegisterUninstaller: Boolean;
var I: Integer;
begin
  Result := True;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/NOREGISTRY') = 0 then Result := False;
end;