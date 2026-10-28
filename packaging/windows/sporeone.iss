; The Windows installer, built with Inno Setup 6:
;   iscc /DVersion=0.0.1 /DBuild=target\release packaging\windows\sporeone.iss
; It installs for the current user, without administrator rights, and updates an installation in
; place. SporeOne's own data (the catalog it downloaded, downloads, which release each mod came
; from) is in %LOCALAPPDATA%\SporeOne; the uninstaller asks whether to delete it. Installed mods
; belong to the Launcher Kit and stay.

#ifndef Version
  #error Pass the version: /DVersion=x.y.z
#endif
#ifndef Build
  #define Build "target\release"
#endif
#define Root "..\.."

[Setup]
AppId={{B7E2C4D9-5A31-4F6B-8E0C-3D91A7F25C64}
AppName=SporeOne
AppVersion={#Version}
AppVerName=SporeOne {#Version}
AppPublisher=Brunovncs
AppPublisherURL=https://github.com/Brunovncs/SporeOne
AppSupportURL=https://github.com/Brunovncs/SporeOne/issues
AppUpdatesURL=https://github.com/Brunovncs/SporeOne/releases/latest
VersionInfoVersion={#Version}.0
DefaultDirName={localappdata}\Programs\sporeone
DisableDirPage=auto
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
SetupIconFile={#Root}\assets\icon.ico
UninstallDisplayIcon={app}\sporeone.exe
UninstallDisplayName=SporeOne
CloseApplications=yes
RestartApplications=no
Compression=lzma2/max
SolidCompression=yes
OutputDir={#Root}\dist
OutputBaseFilename=sporeone-{#Version}-windows-x64-setup
LicenseFile={#Root}\LICENSE

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
en.RemoveData=Do you also want to delete SporeOne's downloads and saved catalog?%n%nYour installed mods are kept: they belong to the Launcher Kit.
pt.RemoveData=Deseja também apagar os downloads e o catálogo salvo do SporeOne?%n%nOs mods instalados ficam: eles pertencem ao Launcher Kit.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Root}\{#Build}\sporeone.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\SporeOne"; Filename: "{app}\sporeone.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\SporeOne"; Filename: "{app}\sporeone.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
; Also after a silent update, so the SporeOne that was open comes back.
Filename: "{app}\sporeone.exe"; Description: "{cm:LaunchProgram,SporeOne}"; Flags: nowait postinstall

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Data: String;
begin
  Data := ExpandConstant('{localappdata}\SporeOne');
  if (CurUninstallStep = usPostUninstall) and DirExists(Data) and not UninstallSilent and
    (MsgBox(CustomMessage('RemoveData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    DelTree(Data, True, True, True);
end;
