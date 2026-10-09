; Gym App installer (Inno Setup 6). Build with installer\build.ps1, or open this
; file in the Inno Setup Compiler after building both services in release mode.
;
; Layout
;   {app}\face-service\   face-service.exe, onnxruntime.dll, models\
;   {app}\gym-server\     gym-server.exe
;   {commonappdata}\GymApp\face-service\   face-service.toml + data (faces, events, logs)
;   {commonappdata}\GymApp\gym-server\     gym-server.toml + data (gym.db, snapshots, logs)
;
; Programs and data are kept apart: upgrading or uninstalling never touches the
; data in ProgramData.

#define AppName "Gym App"
#define AppVersion "1.0.0"
#define AppPublisher "Gym App (open source)"
#define AppUrl "https://github.com/mbpmohsen/gym-app"
#define Root ".."
; onnxruntime 1.22.x (x64); Windows' own System32 copy is too old
#define OrtDir Root + "\face-service"

[Setup]
AppId={{6B0C1F52-6E4B-4F0E-9E61-3B7C2C1A9D47}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
DefaultDirName={autopf}\GymApp
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=Output
OutputBaseFilename=GymApp-Setup-{#AppVersion}
SetupIconFile=assets\gym-app.ico
UninstallDisplayIcon={app}\gym-app.ico
UninstallDisplayName={#AppName}
WizardStyle=modern
WizardSizePercent=110
WizardImageFile=assets\wizard-100.bmp,assets\wizard-150.bmp,assets\wizard-200.bmp
WizardSmallImageFile=assets\wizard-small-100.bmp,assets\wizard-small-150.bmp,assets\wizard-small-200.bmp
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=no
ShowLanguageDialog=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Messages]
WelcomeLabel2=This will install [name/ver] on your computer.%n%nGym App runs entirely on this PC: members, subscriptions and entries are stored locally, and members are recognized by the webcam at the door. Nothing is sent to the internet.%n%nTwo background services will be installed and started with Windows:%n  • Face Recognition Service (camera)%n  • Gym Management Server (http://127.0.0.1:7470)
FinishedLabel=Setup has finished installing [name].%n%nOpen it from the desktop shortcut. On first launch you will choose the admin password.%n%nYour data is kept in C:\ProgramData\GymApp

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "assets\gym-app.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\face-service\target\release\face-service.exe"; DestDir: "{app}\face-service"; Flags: ignoreversion
Source: "{#OrtDir}\onnxruntime.dll"; DestDir: "{app}\face-service"; Flags: ignoreversion
Source: "{#OrtDir}\lib\onnxruntime_providers_shared.dll"; DestDir: "{app}\face-service"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#Root}\face-service\models\*.onnx"; DestDir: "{app}\face-service\models"; Flags: ignoreversion
Source: "{#Root}\server\target\release\gym-server.exe"; DestDir: "{app}\gym-server"; Flags: ignoreversion
Source: "{#Root}\README.md"; DestDir: "{app}"; DestName: "README.md"; Flags: ignoreversion

[Dirs]
Name: "{commonappdata}\GymApp\face-service"
Name: "{commonappdata}\GymApp\gym-server"

[Icons]
; Edge "app" window: looks like a desktop program (no tabs or address bar)
Name: "{autodesktop}\{#AppName}"; Filename: "{code:EdgePath}"; Parameters: "--app=http://127.0.0.1:7470/"; IconFilename: "{app}\gym-app.ico"; Tasks: desktopicon; Check: HasEdge
Name: "{autodesktop}\{#AppName}"; Filename: "http://127.0.0.1:7470/"; IconFilename: "{app}\gym-app.ico"; Tasks: desktopicon; Check: not HasEdge
Name: "{autoprograms}\{#AppName}"; Filename: "{code:EdgePath}"; Parameters: "--app=http://127.0.0.1:7470/"; IconFilename: "{app}\gym-app.ico"; Check: HasEdge
Name: "{autoprograms}\{#AppName}"; Filename: "http://127.0.0.1:7470/"; IconFilename: "{app}\gym-app.ico"; Check: not HasEdge
Name: "{autoprograms}\{#AppName} data folder"; Filename: "{commonappdata}\GymApp"

[Run]
Filename: "{code:EdgePath}"; Parameters: "--app=http://127.0.0.1:7470/"; Description: "Open {#AppName} now"; Flags: postinstall nowait skipifsilent; Check: HasEdge
Filename: "http://127.0.0.1:7470/"; Description: "Open {#AppName} now"; Flags: postinstall nowait skipifsilent shellexec; Check: not HasEdge

[UninstallRun]
Filename: "{app}\gym-server\gym-server.exe"; Parameters: "uninstall"; Flags: runhidden waituntilterminated; RunOnceId: "UninstallGymServer"
Filename: "{app}\face-service\face-service.exe"; Parameters: "uninstall"; Flags: runhidden waituntilterminated; RunOnceId: "UninstallFaceService"

[Code]
const
  FaceConfig = '\GymApp\face-service\face-service.toml';
  GymConfig = '\GymApp\gym-server\gym-server.toml';

function EdgePath(Param: String): String;
begin
  Result := ExpandConstant('{commonpf32}\Microsoft\Edge\Application\msedge.exe');
  if not FileExists(Result) then
    Result := ExpandConstant('{commonpf64}\Microsoft\Edge\Application\msedge.exe');
end;

function HasEdge: Boolean;
begin
  Result := FileExists(EdgePath(''));
end;

function RandomHex(Len: Integer): String;
var
  I: Integer;
begin
  Result := '';
  for I := 1 to Len do
    Result := Result + Copy('0123456789abcdef', Random(16) + 1, 1);
end;

{ sc.exe: ignore the result (service may not exist) }
procedure Sc(Args: String);
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\sc.exe'), Args, '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

{ Stops and removes services from an earlier install (possibly from another folder),
  so the files can be replaced and the services re-registered with the new paths. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Sc('stop GymServer');
  Sc('stop FaceService');
  Sleep(2500);
  Sc('delete GymServer');
  Sc('delete FaceService');
  Sleep(500);
  Result := '';
end;

{ Configs are written once; an upgrade keeps the existing ones (and the token). }
procedure WriteConfigs;
var
  Data, App, Token: String;
begin
  Data := ExpandConstant('{commonappdata}');
  App := ExpandConstant('{app}');
  if not FileExists(Data + FaceConfig) then
  begin
    Token := RandomHex(32);
    SaveStringToFile(Data + FaceConfig,
      '# face-service. Relative paths are relative to this file.' + #13#10 +
      'bind = "127.0.0.1:7480"' + #13#10 +
      'token = "' + Token + '"' + #13#10 +
      'camera = "auto"' + #13#10 +
      'models_dir = ''' + App + '\face-service\models''' + #13#10 +
      'data_dir = "data"' + #13#10 +
      'onnxruntime = ''' + App + '\face-service\onnxruntime.dll''' + #13#10 +
      #13#10 + '[recognition]' + #13#10 +
      'threshold = 0.363' + #13#10 +
      'cooldown_secs = 60.0' + #13#10 +
      'max_fps = 8.0' + #13#10, False);
  end;
  if not FileExists(Data + GymConfig) then
    SaveStringToFile(Data + GymConfig,
      '# gym-server. Relative paths are relative to this file.' + #13#10 +
      'bind = "127.0.0.1:7470"' + #13#10 +
      'data_dir = "data"' + #13#10 +
      'face_service_url = "http://127.0.0.1:7480"' + #13#10 +
      'face_service_token = ""' + #13#10 +
      'face_service_config = ''' + Data + FaceConfig + '''' + #13#10, False);
end;

function RunTool(Exe, Args: String): Integer;
begin
  if not Exec(Exe, Args, ExtractFileDir(Exe), SW_HIDE, ewWaitUntilTerminated, Result) then
    Result := -1;
end;

procedure InstallService(Exe, Config, Title: String);
begin
  WizardForm.StatusLabel.Caption := 'Starting ' + Title + '...';
  RunTool(Exe, 'install --config "' + Config + '"');
  if RunTool(Exe, 'start') <> 0 then
    MsgBox(Title + ' could not be started.' + #13#10#13#10 +
      'Check the log in ' + ExtractFileDir(Config) + '\data\logs, then start it from Services (services.msc).',
      mbError, MB_OK);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Data, App: String;
begin
  if CurStep = ssPostInstall then
  begin
    Data := ExpandConstant('{commonappdata}');
    App := ExpandConstant('{app}');
    WriteConfigs;
    InstallService(App + '\face-service\face-service.exe', Data + FaceConfig, 'Face Recognition Service');
    InstallService(App + '\gym-server\gym-server.exe', Data + GymConfig, 'Gym Management Server');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    if MsgBox('Also delete all gym data (members, subscriptions, entries, faces)?' + #13#10#13#10 +
        ExpandConstant('{commonappdata}\GymApp') + #13#10#13#10 +
        'Choose No to keep it for a later reinstall.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{commonappdata}\GymApp'), True, True, True);
end;
