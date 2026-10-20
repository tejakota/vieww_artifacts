; Wright for Windows — Inno Setup script.
;
; Built by .github/workflows/wright-release.yml:
;   iscc /DAppVersion=0.3.0 /DDist=<folder with wright.exe, wright.ico, docs> wright.iss
;
; Installs per user (no administrator prompt) into
; %LOCALAPPDATA%\Programs\Wright, with a Start-menu entry, an optional
; desktop icon, and an optional PATH entry so `wright` works in a terminal.
; Uninstalling removes the program, never your projects or ~/.wright.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Dist
  #define Dist "dist"
#endif

[Setup]
AppId={{6B0E8F3C-6E2A-4C7B-9C55-2B5D1F0A7E31}
AppName=Wright
AppVersion={#AppVersion}
AppVerName=Wright {#AppVersion}
AppPublisher=The Wright Authors
AppPublisherURL=https://github.com/tejakota/vieww_artifacts
DefaultDirName={localappdata}\Programs\Wright
DefaultGroupName=Wright
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Dist}\..\out
OutputBaseFilename=Wright-Setup-{#AppVersion}-x64
SetupIconFile={#Dist}\wright.ico
UninstallDisplayIcon={app}\wright.exe
UninstallDisplayName=Wright
Compression=lzma2/ultra
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes

[Tasks]
Name: "desktopicon"; Description: "Put Wright on the desktop"; Flags: unchecked
Name: "addtopath"; Description: "Use wright from a terminal (adds it to PATH)"

[Files]
Source: "{#Dist}\wright.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\wright.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Dist}\examples\*"; DestDir: "{app}\examples"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Wright"; Filename: "{app}\wright.exe"; IconFilename: "{app}\wright.ico"
Name: "{group}\Uninstall Wright"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Wright"; Filename: "{app}\wright.exe"; IconFilename: "{app}\wright.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\wright.exe"; Description: "Open Wright"; Flags: nowait postinstall skipifsilent

[Code]
{ True when `Dir` is not already on the user's PATH (case-insensitive). }
function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;
