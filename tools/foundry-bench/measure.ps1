<#
.SYNOPSIS
Runs foundry-bench and samples NPU and GPU utilization while it runs.

.DESCRIPTION
Device detection is not proof of execution, so this samples the
`\GPU Engine(*)\Utilization Percentage` counters. On a Snapdragon X device the
Hexagon NPU is a separate compute-only adapter (one engine, type "compute"); the
Adreno GPU has 3d, compute, and video engines. Adapters are found by engine shape,
not by a fixed LUID, because LUIDs change across reboots.

.EXAMPLE
./measure.ps1 bench qwen2.5-1.5b-instruct-qnn-npu:3 tidy
#>
param([Parameter(ValueFromRemainingArguments)] [string[]] $BenchArgs)

$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'target\release\foundry-bench.exe'

$engines = (Get-Counter '\GPU Engine(*)\Utilization Percentage').CounterSamples |
    ForEach-Object {
        if ($_.InstanceName -match 'luid_(0x[0-9a-f]+_0x[0-9a-f]+)_phys_\d+_eng_\d+_engtype_(.*)$') {
            [pscustomobject]@{ Luid = $Matches[1]; Type = $Matches[2] }
        }
    } | Sort-Object Luid, Type -Unique | Group-Object Luid
$npu = ($engines | Where-Object { $_.Count -eq 1 -and $_.Group[0].Type -eq 'compute' }).Name
$gpu = ($engines | Where-Object { $_.Group.Type -contains 'videodecode' }).Name
Write-Host "[adapters] npu=$npu gpu=$gpu"

$sampler = Start-Job -ArgumentList $npu, $gpu -ScriptBlock {
    param($npu, $gpu)
    while ($true) {
        $samples = (Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction SilentlyContinue).CounterSamples
        $sum = { param($luid, $type) ($samples | Where-Object { $_.InstanceName -like "*$luid*engtype_$type" } | Measure-Object CookedValue -Sum).Sum }
        [pscustomobject]@{
            Npu = & $sum $npu 'compute'
            GpuCompute = & $sum $gpu 'compute'
            Gpu3d = & $sum $gpu '3d'
        }
    }
}

try {
    & $exe @BenchArgs
} finally {
    Stop-Job $sampler
    $rows = Receive-Job $sampler
    Remove-Job $sampler
    $stat = { param($name) $v = $rows.$name; '{0}: peak {1:N0}% mean {2:N0}%' -f $name, ($v | Measure-Object -Maximum).Maximum, ($v | Measure-Object -Average).Average }
    Write-Host ("[utilization] {0} samples | {1} | {2} | {3}" -f $rows.Count, (& $stat 'Npu'), (& $stat 'GpuCompute'), (& $stat 'Gpu3d'))
}
