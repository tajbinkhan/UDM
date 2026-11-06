<#
Drive one of IDM's windows by handle, with window messages rather than the
mouse or keyboard, so nothing reaches the program in front. Take handles
from watch-idm.ps1's log.

    pwsh -File tools/idm/idm-window.ps1 -Hwnd 0x4083C -Action Click -Id 1006
    pwsh -File tools/idm/idm-window.ps1 -Hwnd 0x1088C -Action Minimize
    pwsh -File tools/idm/idm-window.ps1 -Hwnd 0x1088C -Action Restore
    pwsh -File tools/idm/idm-window.ps1 -Hwnd 0x1088C -Action Capture -Path window.png
    pwsh -File tools/idm/idm-window.ps1 -Hwnd 0x1088C -Action Caption -X 385 -Y 19

Click      presses the dialog button with control ID -Id (BM_CLICK). Start
           Download in "Download File Info" is 1006; decode-dialogs.py lists
           the others.
Minimize   and Restore send SC_MINIMIZE and SC_RESTORE.
Capture    saves the window as a PNG with PrintWindow, even behind another
           window; find IDM's own caption buttons by their pixels in it.
Caption    clicks a button IDM draws in the title bar, at -X, -Y in window
           pixels: WM_NCLBUTTONDOWN there, then WM_LBUTTONUP in client
           coordinates, since IDM captures the mouse on the press. In IDM
           6.43 the download window's green arrow, which hides it to the
           tray, is about 385, 19.
#>
param(
  [Parameter(Mandatory)][string]$Hwnd,
  [Parameter(Mandatory)][ValidateSet('Click', 'Minimize', 'Restore', 'Capture', 'Caption')][string]$Action,
  [int]$Id, [string]$Path, [int]$X, [int]$Y
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class IdmWin {
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@
$h = [IntPtr][Convert]::ToInt64($Hwnd, 16)
function Pack([int]$a, [int]$b) { [IntPtr]((($b -band 0xFFFF) -shl 16) -bor ($a -band 0xFFFF)) }
switch ($Action) {
  'Click' {
    $button = [IdmWin]::GetDlgItem($h, $Id)
    if ($button -eq [IntPtr]::Zero) { throw "no control $Id in that window" }
    [void][IdmWin]::SendMessage($button, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero)
  }
  'Minimize' { [void][IdmWin]::SendMessage($h, 0x112, [IntPtr]0xF020, [IntPtr]::Zero) }
  'Restore' { [void][IdmWin]::SendMessage($h, 0x112, [IntPtr]0xF120, [IntPtr]::Zero) }
  'Capture' {
    $r = New-Object IdmWin+RECT; [void][IdmWin]::GetWindowRect($h, [ref]$r)
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
    [void][IdmWin]::PrintWindow($h, $dc, 2)
    $g.ReleaseHdc($dc); $bmp.Save($Path); $g.Dispose(); $bmp.Dispose()
  }
  'Caption' {
    $r = New-Object IdmWin+RECT; [void][IdmWin]::GetWindowRect($h, [ref]$r)
    $p = New-Object IdmWin+POINT; $p.X = $r.L + $X; $p.Y = $r.T + $Y
    $screen = Pack $p.X $p.Y
    [void][IdmWin]::ScreenToClient($h, [ref]$p)
    [void][IdmWin]::SendMessage($h, 0xA1, [IntPtr]::Zero, $screen)
    Start-Sleep -Milliseconds 150
    [void][IdmWin]::SendMessage($h, 0x202, [IntPtr]::Zero, (Pack $p.X $p.Y))
  }
}
