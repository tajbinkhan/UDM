; Inno Setup script for Pullbay's Windows installer. It installs for the current
; user only, needs no administrator rights, and does what install-host.ps1
; does: the three programs, the extension folder and its Firefox copy, the
; native-messaging host registered with Chrome, Edge and Firefox, and a Start
; menu shortcut.
;
; Build it with installer\build.ps1, which builds the programs and passes the
; version from Cargo.toml. To call the compiler yourself:
;   ISCC.exe /DAppVersion=0.3.0 installer\pullbay.iss

#ifndef AppVersion
  #define AppVersion "0.3.0"
#endif
#define HostName "com.pullbay.host"
; Fixed by the public key in extension\manifest.json.
#define ExtensionId "epmjakacaejhdhldkpbjnakhfmjjbhfm"
; From extension\firefox.json.
#define FirefoxId "pullbay@pullbay.local"
#define Root ".."

[Setup]
AppId={{8C6F2B1E-5D3A-4E7B-9A41-2F0C7D9E6B13}
AppName=Pullbay
AppVersion={#AppVersion}
AppVerName=Pullbay {#AppVersion}
AppPublisher=Tajbin Khan
AppPublisherURL=https://github.com/tajbinkhan/pullbay
AppSupportURL=https://github.com/tajbinkhan/pullbay/issues
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Pullbay setup
; Per user: the browsers read native-messaging hosts from HKCU, and the
; daemon keeps its data under the user's profile. {autopf} is then
; %LOCALAPPDATA%\Programs.
PrivilegesRequired=lowest
#if Ver >= EncodeVer(6, 7, 0)
; Inno Setup 6.7 turns on Windows' RedirectionGuard for Setup, and every
; program Setup starts inherits it: Pullbay from the last page or after Update
; now, then its daemon, then yt-dlp. yt-dlp then fails with WinError 448 on
; a junction a user made, such as pnpm's on PATH. The guard stops junction
; tricks that gain privileges, and this Setup never has more than the user.
RedirectionGuard=no
#endif
DefaultDirName={autopf}\Pullbay
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}\dist
OutputBaseFilename=Pullbay-Setup-{#AppVersion}
SetupIconFile=pullbay.ico
UninstallDisplayIcon={app}\pullbay.ico
UninstallDisplayName=Pullbay
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
; Running Pullbay programs are closed in [Code], after asking.
CloseApplications=no

[Tasks]
Name: desktopicon; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
; Shown only when winget is there and yt-dlp is not. winget installs yt-dlp
; with FFmpeg and Deno, which it depends on.
Name: videotools; Description: "Install yt-dlp, FFmpeg and Deno with winget, for YouTube and other video sites (downloads them now)"; GroupDescription: "Video tools:"; Check: CanInstallVideoTools

[Files]
Source: "{#Root}\target\release\pullbay-host.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\release\pullbay-daemon.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\release\pullbay-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\release\pullbay-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\extension\*"; DestDir: "{app}\extension"; Excludes: "test\*,firefox.json"; Flags: ignoreversion recursesubdirs createallsubdirs
; Made by build.ps1 with make-firefox.ps1.
Source: "{#Root}\firefox\*"; DestDir: "{app}\firefox"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "pullbay.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
; The browsers find pullbay-host.exe through the manifest these keys name. The
; manifest itself is written in [Code], with this install's path in it.
Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\{#HostName}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#HostName}.json"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Edge\NativeMessagingHosts\{#HostName}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#HostName}.json"; Flags: uninsdeletekey
; Firefox names the allowed extension by ID, so it has its own manifest.
Root: HKCU; Subkey: "Software\Mozilla\NativeMessagingHosts\{#HostName}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#HostName}.firefox.json"; Flags: uninsdeletekey
; Options can register other browsers; uninstalling removes those too.
Root: HKCU; Subkey: "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\{#HostName}"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Chromium\NativeMessagingHosts\{#HostName}"; Flags: uninsdeletekey dontcreatekey

[Icons]
Name: "{autoprograms}\Pullbay"; Filename: "{app}\pullbay-gui.exe"; WorkingDir: "{app}"; IconFilename: "{app}\pullbay.ico"; Comment: "Pullbay download manager"
Name: "{autodesktop}\Pullbay"; Filename: "{app}\pullbay-gui.exe"; WorkingDir: "{app}"; IconFilename: "{app}\pullbay.ico"; Comment: "Pullbay download manager"; Tasks: desktopicon

[Run]
Filename: "{app}\pullbay-gui.exe"; Description: "{cm:LaunchProgram,Pullbay}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\extension"; Description: "Open the extension folder, to load it in the browser"; Flags: shellexec nowait postinstall skipifsilent unchecked

[InstallDelete]
; UIDM's shortcuts, from before Pullbay got its name.
Type: files; Name: "{autoprograms}\UIDM.lnk"
Type: files; Name: "{autodesktop}\UIDM.lnk"

[UninstallDelete]
Type: files; Name: "{app}\{#HostName}.json"
Type: files; Name: "{app}\{#HostName}.firefox.json"
Type: files; Name: "{localappdata}\Pullbay\{#HostName}.json"
Type: files; Name: "{localappdata}\Pullbay\{#HostName}.firefox.json"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  { Written by the daemon so Windows shows Pullbay's notifications. }
  NotificationKey = 'Software\Classes\AppUserModelId\Pullbay.DownloadManager';

var
  { The folder UIDM, as Pullbay was called up to 0.3.3, was installed in,
    when this Setup updates it; empty otherwise. }
  OldDir: String;

{ An update of UIDM installs into a folder named Pullbay beside UIDM's,
  which goes once its programs are closed. Pullbay's own updates keep the
  folder they were in. }
procedure InitializeWizard();
var
  Dir: String;
begin
  Dir := RemoveBackslash(WizardForm.DirEdit.Text);
  if CompareText(ExtractFileName(Dir), 'UIDM') = 0 then
  begin
    OldDir := Dir;
    WizardForm.DirEdit.Text := AddBackslash(ExtractFileDir(Dir)) + 'Pullbay';
  end;
end;

{ A JSON string literal. Characters outside ASCII become \u escapes, so the
  file is plain ASCII whatever the user's folder is called. }
function JsonString(S: String): String;
var
  I: Integer;
  C: Char;
begin
  Result := '"';
  for I := 1 to Length(S) do
  begin
    C := S[I];
    if C = '\' then
      Result := Result + '\\'
    else if C = '"' then
      Result := Result + '\"'
    else if (Ord(C) < 32) or (Ord(C) > 126) then
      Result := Result + Format('\u%.4x', [Ord(C)])
    else
      Result := Result + C;
  end;
  Result := Result + '"';
end;

{ A native-messaging manifest: where pullbay-host.exe is, and the only
  extension allowed to start it. Allowed is the last line, such as
  "allowed_origins": [...]. }
procedure WriteHostManifest(FileName, Allowed: String);
var
  Json, Path: String;
begin
  Json :=
    '{' + #13#10 +
    '  "name": "{#HostName}",' + #13#10 +
    '  "description": "Pullbay download manager native-messaging host",' + #13#10 +
    '  "type": "stdio",' + #13#10 +
    '  "path": ' + JsonString(ExpandConstant('{app}\pullbay-host.exe')) + ',' + #13#10 +
    '  ' + Allowed + #13#10 +
    '}' + #13#10;
  Path := ExpandConstant('{app}\') + FileName;
  if not SaveStringToFile(Path, Json, False) then
    RaiseException('Could not write ' + Path);
end;

{ Find the Pullbay programs running from this install's folder, and UIDM's
  from the folder this Setup updates, and with Stop end them. True when any
  were running. Programs of the same name elsewhere (a development build)
  are left alone. }
function PullbayRunning(Stop: Boolean): Boolean;
var
  Dir, Old, Script: String;
  Code: Integer;
begin
  Dir := AddBackslash(ExpandConstant('{app}'));
  StringChangeEx(Dir, '''', '''''', True);
  Old := '';
  if OldDir <> '' then
    Old := AddBackslash(OldDir);
  StringChangeEx(Old, '''', '''''', True);
  Script :=
    '$d = ''' + Dir + '''; $o = ''' + Old + '''; ' +
    '$p = @(Get-Process pullbay-gui,pullbay-daemon,pullbay-host,pullbay-cli,uidm-gui,uidm-daemon,uidm-host -ErrorAction SilentlyContinue | ' +
    'Where-Object { $_.Path -and ($_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) -or ' +
    '($o -and $_.Path.StartsWith($o, [StringComparison]::OrdinalIgnoreCase))) }); ' +
    'if ($p.Count -eq 0) { exit 0 }; ';
  if Stop then
    Script := Script + '$p | ForEach-Object { $_.Kill(); [void]$_.WaitForExit(5000) }; ';
  Script := Script + 'exit 1';
  Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "' + Script + '"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
  Result := Code = 1;
end;

{ Close a running Pullbay after asking. False when the user says no. }
function ClosePullbay(Silent: Boolean): Boolean;
begin
  Result := True;
  if not PullbayRunning(False) then
    exit;
  if not Silent then
    Result := MsgBox('Pullbay is running. It will be closed first.' + #13#10#13#10 +
      'Downloads in progress stop and can be resumed from Pullbay''s list.',
      mbConfirmation, MB_OKCANCEL) = IDOK;
  if Result then
    PullbayRunning(True);
end;

{ Started by Pullbay's Update now (/frompullbay=yes), or UIDM's
  (/fromuidm=yes): the user already agreed to close it. }
function FromPullbay(): Boolean;
begin
  Result := (ExpandConstant('{param:frompullbay|no}') = 'yes') or
    (ExpandConstant('{param:fromuidm|no}') = 'yes');
end;

function SetEnvironmentVariable(Name, Value: String): BOOL;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';
function ExpandEnvironmentStrings(Src: String; Dst: String; Size: DWORD): DWORD;
  external 'ExpandEnvironmentStringsW@kernel32.dll stdcall';

{ winget, through the alias Windows 10 1809 and later put in WindowsApps. }
function WingetPath(): String;
begin
  Result := ExpandConstant('{localappdata}\Microsoft\WindowsApps\winget.exe');
end;

function YtDlpFound(): Boolean;
begin
  Result := FileSearch('yt-dlp.exe', GetEnv('PATH')) <> '';
end;

{ The video tools task is offered when winget can install yt-dlp and it is
  not installed yet. }
function CanInstallVideoTools(): Boolean;
begin
  Result := FileExists(WingetPath()) and not YtDlpFound();
end;

{ %VARS% in S filled in. }
function Expand(S: String): String;
var
  Size: DWORD;
begin
  Result := S;
  if Pos('%', S) = 0 then
    exit;
  Size := ExpandEnvironmentStrings(S, '', 0);
  if Size = 0 then
    exit;
  SetLength(Result, Size);
  if ExpandEnvironmentStrings(S, Result, Size) = 0 then
    Result := S
  else
    SetLength(Result, Size - 1);
end;

{ winget puts the tools on the user's PATH in the registry. Setup's own PATH
  is the one it started with, and Pullbay started from the last page inherits
  it, so read the new one in. }
procedure RefreshPath();
var
  Machine, User, Path: String;
begin
  RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', Machine);
  RegQueryStringValue(HKCU, 'Environment', 'Path', User);
  Path := Expand(Machine);
  if User <> '' then
  begin
    if (Path <> '') and (Path[Length(Path)] <> ';') then
      Path := Path + ';';
    Path := Path + Expand(User);
  end;
  if Path <> '' then
    SetEnvironmentVariable('PATH', Path);
end;

procedure InstallVideoTools();
var
  Code: Integer;
  Ok: Boolean;
begin
  WizardForm.StatusLabel.Caption := 'Installing yt-dlp, FFmpeg and Deno with winget. This can take a few minutes...';
  WizardForm.FilenameLabel.Caption := '';
  WizardForm.ProgressGauge.Style := npbstMarquee;
  try
    Ok := Exec(WingetPath(),
      'install --id yt-dlp.yt-dlp --exact --source winget --silent --disable-interactivity ' +
      '--accept-package-agreements --accept-source-agreements',
      '', SW_HIDE, ewWaitUntilTerminated, Code);
  finally
    WizardForm.ProgressGauge.Style := npbstNormal;
  end;
  RefreshPath();
  if not (Ok and YtDlpFound()) then
    SuppressibleMsgBox('winget could not install the video tools (exit code ' + IntToStr(Code) + ').' + #13#10#13#10 +
      'Pullbay still downloads ordinary files. For YouTube and other video sites, run this later:' + #13#10 +
      '    winget install yt-dlp.yt-dlp',
      mbError, MB_OK, IDOK);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not ClosePullbay(WizardSilent or FromPullbay()) then
    Result := 'Pullbay is still running. Close it and run Setup again.';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    WriteHostManifest('{#HostName}.json', '"allowed_origins": ["chrome-extension://{#ExtensionId}/"]');
    WriteHostManifest('{#HostName}.firefox.json', '"allowed_extensions": ["{#FirefoxId}"]');
    { UIDM's programs are closed by now. Its settings, list, browser
      registrations and sign-in entry move when Pullbay first starts. }
    if (OldDir <> '') and DirExists(OldDir) then
      DelTree(OldDir, True, True, True);
    if WizardIsTaskSelected('videotools') then
      InstallVideoTools();
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
var
  Video: String;
begin
  if CurPageID <> wpFinished then
    exit;
  if YtDlpFound() then
    Video := ''
  else
    Video := #13#10#13#10 +
      'For YouTube and other video sites, install yt-dlp, which brings FFmpeg and Deno with it: ' +
      'winget install yt-dlp.yt-dlp';
  if OldDir <> '' then
    Video := #13#10#13#10 +
      'UIDM is now called Pullbay. Remove the UIDM extension from the browser first: ' +
      'it went with UIDM''s folder, and Pullbay''s takes its place.' + Video;
  WizardForm.FinishedLabel.Caption :=
      'Pullbay is installed. To catch downloads in Chrome or Edge, load its extension once:' + #13#10#13#10 +
      '1. Open chrome://extensions (or edge://extensions) and turn on Developer mode.' + #13#10 +
      '2. Click Load unpacked and choose this folder:' + #13#10 +
      '    ' + ExpandConstant('{app}\extension') + #13#10 +
      '3. Check that its ID reads {#ExtensionId}.' + #13#10#13#10 +
      'In Firefox, open about:debugging, choose This Firefox, then Load Temporary Add-on, ' +
      'and pick manifest.json in ' + ExpandConstant('{app}\firefox') + '.' + Video;
end;

function InitializeUninstall(): Boolean;
begin
  Result := ClosePullbay(UninstallSilent);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep <> usPostUninstall then
    exit;
  RegDeleteKeyIncludingSubkeys(HKCU, NotificationKey);
  { "Start with Windows", when it points at this install. }
  if RegQueryStringValue(HKCU, RunKey, 'Pullbay', Command) and
     (Pos(Lowercase(ExpandConstant('{app}')), Lowercase(Command)) > 0) then
    RegDeleteValue(HKCU, RunKey, 'Pullbay');
  if not UninstallSilent and
     (MsgBox('Also remove your Pullbay settings and download list?' + #13#10#13#10 +
       'Downloaded files are kept either way.',
       mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
  begin
    DelTree(ExpandConstant('{userappdata}\Pullbay'), True, True, True);
    DelTree(ExpandConstant('{localappdata}\Pullbay'), True, True, True);
  end;
end;
