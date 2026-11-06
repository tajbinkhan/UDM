<#
Log IDM's top-level windows while it works, for comparing UIDM with it.

    pwsh -File tools/idm/watch-idm.ps1 -Log idm-log.txt [-Seconds 900]

Every half second it records each IDM window's handle, class, title,
visibility, minimized and foreground state, style and extended style (with
the taskbar flags WS_EX_APPWINDOW and WS_EX_TOOLWINDOW read out), owner,
position and the text of its visible controls. A line is written only when
something other than a number changes, so a download's progress does not
flood the log. Every two seconds it also writes the names of the taskbar and
visible tray buttons that mention IDM or a download; icons in the hidden
tray area are not seen. WS_EX_TOPMOST is the 0x8 bit of exstyle.

Run it in the background, then make IDM act: catch a download in the
browser, or drive its windows with idm-window.ps1.
#>
param([int]$IdmPid = 0, [Parameter(Mandatory)][string]$Log, [int]$Seconds = 900)
if (-not $IdmPid) {
  $p = Get-Process IDMan -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { throw "IDM (IDMan.exe) is not running" }
  $IdmPid = $p.Id
}
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class W {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint c);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static string Text(IntPtr h) { var s = new StringBuilder(512); GetWindowText(h, s, 512); return s.ToString(); }
  public static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
  public static List<string> Snapshot(uint pid) {
    var rows = new List<string>();
    IntPtr fg = GetForegroundWindow();
    EnumWindows((h, p) => {
      uint wp; GetWindowThreadProcessId(h, out wp);
      if (wp != pid) return true;
      string t = Text(h);
      bool vis = IsWindowVisible(h);
      if (!vis && t.Length == 0) return true;
      int st = GetWindowLong(h, -16), ex = GetWindowLong(h, -20);
      RECT r; GetWindowRect(h, out r);
      IntPtr owner = GetWindow(h, 4);
      var kids = new List<string>();
      if (vis) EnumChildWindows(h, (c, q) => { if (IsWindowVisible(c)) { string ct = Text(c); if (ct.Length > 0 && ct.Length < 90) kids.Add(ct); } return true; }, IntPtr.Zero);
      rows.Add(string.Format("hwnd={0:X} class={1} title=\"{2}\" visible={3} iconic={4} fg={5} style={6:X8} exstyle={7:X8} appwindow={8} toolwindow={9} owner={10:X} rect={11},{12},{13}x{14} controls=[{15}]",
        h.ToInt64(), Cls(h), t, vis, IsIconic(h), h == fg, st, ex, (ex & 0x40000) != 0, (ex & 0x80) != 0, owner.ToInt64(), r.L, r.T, r.R - r.L, r.B - r.T, string.Join(" | ", kids)));
      return true;
    }, IntPtr.Zero);
    rows.Sort();
    return rows;
  }
}
"@
function TrayNames {
  $names = @()
  try {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, "Shell_TrayWnd")
    $tray = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
    if ($tray) {
      $btn = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
      foreach ($b in $tray.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btn)) {
        $n = $b.Current.Name
        if ($n -match 'IDM|Internet Download|Download|%|MB|GB|KB') { $names += $n }
      }
    }
  } catch { $names += "UIA error: $_" }
  return ($names -join ' || ')
}
$prev = ""; $prevTray = ""; $start = Get-Date; $i = 0
"started $(Get-Date -Format o) pid=$IdmPid" | Out-File -FilePath $Log -Encoding utf8
while (((Get-Date) - $start).TotalSeconds -lt $Seconds) {
  $snap = ([W]::Snapshot([uint32]$IdmPid)) -join "`n"
  # The title carries the percentage; compare without digits so progress alone does not log every tick.
  $key = $snap -replace '\d', '#'
  $t = "{0,6:N1}s" -f ((Get-Date) - $start).TotalSeconds
  if ($key -ne $prev) { "[$t] WINDOWS`n$snap" | Out-File -FilePath $Log -Append -Encoding utf8; $prev = $key }
  if ($i % 4 -eq 0) {
    $tr = TrayNames
    $trKey = $tr -replace '\d', '#'
    if ($trKey -ne $prevTray) { "[$t] TRAY $tr" | Out-File -FilePath $Log -Append -Encoding utf8; $prevTray = $trKey }
  }
  $i++
  Start-Sleep -Milliseconds 500
}
"stopped $(Get-Date -Format o)" | Out-File -FilePath $Log -Append -Encoding utf8
