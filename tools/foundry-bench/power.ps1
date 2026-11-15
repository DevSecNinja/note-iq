<#
.SYNOPSIS
Rough power comparison on battery: samples BatteryStatus.DischargeRate (mW) while idle and
while running a foundry-bench command. Whole-system draw, so treat it as an indication only.
#>
param([int] $IdleSeconds = 20, [Parameter(ValueFromRemainingArguments)] [string[]] $BenchArgs)

$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'target\release\foundry-bench.exe'
$rate = { (Get-CimInstance -Namespace root\wmi -ClassName BatteryStatus).DischargeRate }
if (-not (Get-CimInstance -Namespace root\wmi -ClassName BatteryStatus).Discharging) {
    throw 'Unplug the charger: discharge rate is only reported on battery.'
}

$idle = 1..$IdleSeconds | ForEach-Object { & $rate; Start-Sleep 1 }
$proc = Start-Process -PassThru -NoNewWindow -FilePath $exe -ArgumentList $BenchArgs -RedirectStandardOutput "$env:TEMP\fb-power.txt" -RedirectStandardError "$env:TEMP\fb-power.err"
$busy = while (-not $proc.HasExited) { & $rate; Start-Sleep 1 }
$avg = { param($v) [math]::Round((($v | Measure-Object -Average).Average) / 1000, 1) }
Write-Host ("[power] idle {0} W | during run {1} W (peak {2} W) over {3} s" -f (& $avg $idle), (& $avg $busy), [math]::Round((($busy | Measure-Object -Maximum).Maximum) / 1000, 1), $busy.Count)
