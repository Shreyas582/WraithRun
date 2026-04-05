[CmdletBinding()]
param(
    [string]$ModelRoot = "C:\models",
    [string]$OutputPath = "launch-assets/generated/live-smoke-matrix.json",
    [string]$FeatureFlag = "inference_bridge/vitis",
    [string]$OrtDylibPath = "C:\Program Files\RyzenAI\1.7.1\onnxruntime\bin\onnxruntime.dll",
    [string]$VitisConfig = "C:\Program Files\RyzenAI\1.7.1\voe-4.0-win_amd64\vaip_config.json",
    [int]$MaxPacks = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot {
    $scriptDir = Split-Path -Parent $PSCommandPath
    return (Resolve-Path (Join-Path $scriptDir "..")).Path
}

function Resolve-OutputPath {
    param(
        [string]$Candidate,
        [string]$RepoRoot
    )

    if ([System.IO.Path]::IsPathRooted($Candidate)) {
        return $Candidate
    }

    return (Join-Path $RepoRoot $Candidate)
}

function Find-ModelPath {
    param([string]$PackPath)

    $preferredNames = @("fusion.onnx", "model.onnx", "model_jit.onnx")
    foreach ($name in $preferredNames) {
        $candidate = Join-Path $PackPath $name
        if (Test-Path -Path $candidate -PathType Leaf) {
            return $candidate
        }
    }

    $topLevel = Get-ChildItem -Path $PackPath -Filter *.onnx -File -ErrorAction SilentlyContinue |
        Sort-Object Name |
        Select-Object -First 1
    if ($null -ne $topLevel) {
        return $topLevel.FullName
    }

    $recursive = Get-ChildItem -Path $PackPath -Filter *.onnx -File -Recurse -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -First 1
    if ($null -ne $recursive) {
        return $recursive.FullName
    }

    return $null
}

function Find-TokenizerPath {
    param([string]$PackPath)

    $topLevel = Join-Path $PackPath "tokenizer.json"
    if (Test-Path -Path $topLevel -PathType Leaf) {
        return $topLevel
    }

    $recursive = Get-ChildItem -Path $PackPath -Filter tokenizer.json -File -Recurse -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -First 1
    if ($null -ne $recursive) {
        return $recursive.FullName
    }

    return $null
}

function Extract-JsonPayload {
    param([string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return $null
    }

    $start = $Text.IndexOf("{")
    $end = $Text.LastIndexOf("}")
    if ($start -lt 0 -or $end -lt $start) {
        return $null
    }

    return $Text.Substring($start, ($end - $start + 1))
}

function Get-Readiness {
    param($Summary)

    if ($null -eq $Summary) {
        return "error"
    }

    if (($Summary.fail | ForEach-Object { [int]$_ }) -gt 0) {
        return "fail"
    }

    if (($Summary.warn | ForEach-Object { [int]$_ }) -gt 0) {
        return "warn"
    }

    return "pass"
}

$repoRoot = Resolve-RepoRoot
$outputFile = Resolve-OutputPath -Candidate $OutputPath -RepoRoot $repoRoot
$outputDir = Split-Path -Parent $outputFile
if (-not [string]::IsNullOrWhiteSpace($outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

if (-not (Test-Path -Path $ModelRoot -PathType Container)) {
    throw "ModelRoot not found: $ModelRoot"
}

if (Test-Path -Path $OrtDylibPath -PathType Leaf) {
    $env:WRAITHRUN_ORT_DYLIB_PATH = $OrtDylibPath
}

$packDirs = Get-ChildItem -Path $ModelRoot -Directory | Sort-Object Name
if ($MaxPacks -gt 0) {
    $packDirs = $packDirs | Select-Object -First $MaxPacks
}

$results = New-Object System.Collections.Generic.List[object]

Push-Location $repoRoot
try {
    foreach ($pack in $packDirs) {
        $modelPath = Find-ModelPath -PackPath $pack.FullName
        $tokenizerPath = Find-TokenizerPath -PackPath $pack.FullName

        if ($null -eq $modelPath -or $null -eq $tokenizerPath) {
                $results.Add([pscustomobject][ordered]@{
                    pack = $pack.Name
                    model = $modelPath
                    tokenizer = $tokenizerPath
                    exit_code = $null
                    readiness = "fail"
                    summary = [ordered]@{
                        pass = 0
                        warn = 0
                        fail = 1
                    }
                    reason_codes = @("model_or_tokenizer_missing")
                    duration_ms = 0
                    parse_error = $null
                })
            continue
        }

        $cargoArgs = @(
            "run", "--quiet", "-p", "wraithrun", "--features", $FeatureFlag,
            "--",
            "--doctor",
            "--live",
            "--model", $modelPath,
            "--tokenizer", $tokenizerPath,
            "--introspection-format", "json"
        )

        if (Test-Path -Path $VitisConfig -PathType Leaf) {
            $cargoArgs += @("--vitis-config", $VitisConfig)
        }

        $started = [System.Diagnostics.Stopwatch]::StartNew()
        $previousErrorAction = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            $rawOutput = (& cargo @cargoArgs 2>&1 | Out-String)
            $exitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorAction
            $started.Stop()
        }

        $jsonText = Extract-JsonPayload -Text $rawOutput
        $parsed = $null
        $parseError = $null

        if ($null -ne $jsonText) {
            try {
                $parsed = $jsonText | ConvertFrom-Json
            }
            catch {
                $parseError = $_.Exception.Message
            }
        }
        else {
            $parseError = "doctor output did not contain a JSON object"
        }

        $summary = $null
        $reasonCodes = @()
        if ($null -ne $parsed) {
            $summary = $parsed.summary
            if ($null -ne $parsed.checks) {
                $reasonCodes = @(
                    $parsed.checks |
                    Where-Object {
                        $_.PSObject.Properties.Match("reason_code").Count -gt 0 -and
                        $null -ne $_.reason_code -and
                        -not [string]::IsNullOrWhiteSpace([string]$_.reason_code)
                    } |
                    ForEach-Object { [string]$_.reason_code } |
                    Sort-Object -Unique
                )
            }
        }

        $results.Add([pscustomobject][ordered]@{
                pack = $pack.Name
                model = $modelPath
                tokenizer = $tokenizerPath
                exit_code = $exitCode
                readiness = Get-Readiness -Summary $summary
                summary = if ($null -eq $summary) {
                    [ordered]@{ pass = 0; warn = 0; fail = 0 }
                }
                else {
                    [ordered]@{
                        pass = [int]$summary.pass
                        warn = [int]$summary.warn
                        fail = [int]$summary.fail
                    }
                }
                reason_codes = $reasonCodes
                duration_ms = [int]$started.ElapsedMilliseconds
                parse_error = $parseError
            })
    }
}
finally {
    Pop-Location
}

$payload = [ordered]@{
    contract_version = "1.0.0"
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    repo_root = $repoRoot
    model_root = $ModelRoot
    feature_flag = $FeatureFlag
    ort_dylib_path = if (Test-Path -Path $OrtDylibPath -PathType Leaf) { $OrtDylibPath } else { $null }
    vitis_config = if (Test-Path -Path $VitisConfig -PathType Leaf) { $VitisConfig } else { $null }
    pack_count = $results.Count
    packs = $results
}

$payload | ConvertTo-Json -Depth 12 | Set-Content -Path $outputFile -Encoding UTF8

$results |
Select-Object pack, readiness, exit_code,
@{ Name = "pass"; Expression = { $_.summary.pass } },
@{ Name = "warn"; Expression = { $_.summary.warn } },
@{ Name = "fail"; Expression = { $_.summary.fail } },
@{ Name = "duration_ms"; Expression = { $_.duration_ms } } |
Format-Table -AutoSize |
Out-String |
Write-Output

Write-Output "Wrote live smoke matrix: $outputFile"
