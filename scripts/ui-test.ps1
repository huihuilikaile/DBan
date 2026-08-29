param([string]$Action)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class M {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
}
'@
function ClickAt([int]$x, [int]$y) {
  [M]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 120
  [M]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 60
  [M]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
}
switch ($Action) {
  "capsule" { ClickAt 2414 60 }      # 面板头部「—」收起为胶囊
  "hover"   { [M]::SetCursorPos(2383, 42) | Out-Null }  # 悬停到胶囊上
  "away"    { [M]::SetCursorPos(1280, 700) | Out-Null } # 移开光标
  "pill"    { ClickAt 2383 42 }      # 点击胶囊展开面板
  "play"    { ClickAt 2466 758 }     # 迷你播放条 播放/暂停
  default   { Write-Output "usage: -Action capsule|hover|away|pill|play" }
}
