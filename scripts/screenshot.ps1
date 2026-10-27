# Takes the window's screenshots from the made-up game (`--demo`), dark theme, for the README:
# docs/catalog.png, docs/mod.png (a mod's page), docs/installed.png, and docs/catalog-pt.png in
# Portuguese. Run it after any change to the window.
#
#     .\scripts\screenshot.ps1
param(
    [int]$Width = 1180,
    [int]$Height = 820
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
# From inside the repository: rustup picks the toolchain by the current folder, not the manifest.
Push-Location $root
try { cargo build -q -p sporeone } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw "build failed" }
$exe = Join-Path $root "target\debug\sporeone.exe"

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class ShotWin {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  public struct RECT { public int L, T, R, B; }
}
"@
[ShotWin]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

function Shot([string]$lang, [string]$page, [string]$out) {
    $env:SPOREONE_THEME = "dark"
    $env:SPOREONE_LANG = $lang
    if ($page) { $env:SPOREONE_PAGE = $page } else { Remove-Item Env:SPOREONE_PAGE -ErrorAction SilentlyContinue }
    Get-Process sporeone -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 300
    $p = Start-Process -FilePath $exe -ArgumentList "--demo" -PassThru
    $h = [IntPtr]::Zero
    for ($i = 0; $i -lt 100 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh(); $h = $p.MainWindowHandle }
    if ($h -eq [IntPtr]::Zero) { throw "the window did not open" }
    $scale = [ShotWin]::GetDpiForWindow($h) / 96.0
    $wr = New-Object ShotWin+RECT; $cr = New-Object ShotWin+RECT
    [ShotWin]::GetWindowRect($h, [ref]$wr) | Out-Null; [ShotWin]::GetClientRect($h, [ref]$cr) | Out-Null
    $dw = ($wr.R - $wr.L) - ($cr.R - $cr.L); $dh = ($wr.B - $wr.T) - ($cr.B - $cr.T)
    [ShotWin]::SetWindowPos($h, [IntPtr]::Zero, 60, 60, [int]($Width * $scale) + $dw, [int]($Height * $scale) + $dh, 0x0014) | Out-Null
    Start-Sleep -Milliseconds 2500
    $r = New-Object ShotWin+RECT
    [ShotWin]::GetClientRect($h, [ref]$r) | Out-Null
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $dc = $g.GetHdc()
    # Client area only, drawn even if another window covers it.
    [ShotWin]::PrintWindow($h, $dc, 3) | Out-Null
    $g.ReleaseHdc($dc)
    $file = Join-Path $root "docs\$out"
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    "$file  $($bmp.Width)x$($bmp.Height)"
    Stop-Process -Id $p.Id -Force
}

Shot "en" "" "catalog.png"
Shot "en" "WoodyWizard/SporeCoop" "mod.png"
Shot "en" "installed" "installed.png"
Shot "pt" "" "catalog-pt.png"
