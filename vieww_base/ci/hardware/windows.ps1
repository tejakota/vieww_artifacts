# Everything a Windows PC with a real GPU and a desktop can close, in one
# unattended run. The Linux twin is `ci/hardware/linux.sh`; both write their
# results in the same shape (receipts, baseline reports, summary.txt).
#
#   powershell -ExecutionPolicy Bypass -File ci\hardware\windows.ps1
#   powershell -ExecutionPolicy Bypass -File ci\hardware\windows.ps1 -Steps baseline,stress
#
# Steps, and what each closes (PENDING.md / the audit's P0 list):
#   env       the machine, GPU, driver, power state and toolchain, recorded first
#   gpu       §1.1/§1.3  Vulkan suites, vieww-3d GPU parity and a GPU frame in a
#                   real window, on this PC's own driver
#   d3d12     §1.3  the D3D12 suite on a HARDWARE adapter (CI only ever had
#                   WARP). Correctness/bring-up only: the D3D12 backend does not
#                   execute a ScenePlan yet, so there is no D3D12 frame to time
#   gallery   §1.1 §2.3 §4.1  CPU gallery + GPU census (Vulkan), 30 runs each,
#                   with receipts and planner shadow logs
#   baseline  P0    a real window, launched N times on the CPU path and N on the
#                   GPU path: launch-to-first-frame, working set, idle frames and
#                   CPU, animated fps, frame-stage medians (examples/baseline.rs).
#                   Energy is not measured on Windows and is reported as null
#   size      P0    size of that windowed app as shipped (`--profile dist`)
#   stress    —     the animation stress suite five times: one reproducible p95
#   desktop   —     the on-desktop suite, through Git Bash
#   remote    §3.3  the studio's out-of-process preview tests
#
# Needs: the pinned Rust toolchain (MSVC), Visual Studio Build Tools, the GPU
# vendor's driver (it ships the Vulkan driver; the Vulkan SDK adds vulkaninfo),
# Git for Windows (bash, for `desktop`), Python 3 for the baseline summary.
# Run from an interactive desktop session: a GitHub runner installed as a
# Windows *service* runs in session 0 and cannot open a window — start it with
# `run.cmd` from a logged-in desktop instead.
#
# Every step runs even when an earlier one failed. Exit status is non-zero if
# any step failed; a step this machine cannot run is SKIP, never PASS.

param(
  [string[]]$Steps = @('env','gpu','d3d12','gallery','baseline','size','stress','desktop','remote'),
  [string]$OutDir = '',
  [int]$Launches = 5
)

$ErrorActionPreference = 'Continue'
# `-File script.ps1 -Steps a,b` hands the script ONE string, "a,b" — the array
# syntax only works when the script is invoked from inside PowerShell. Split
# either form the same way so both mean what they say.
$Steps = @($Steps | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not $OutDir) { $OutDir = Join-Path $Root "hardware-results\windows-$stamp" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$Summary = Join-Path $OutDir 'summary.txt'
$script:Failed = $false

function Want([string]$Step) { return $Steps -contains $Step }

function Say([string]$Line) {
  Write-Host $Line
  Add-Content -Path $Summary -Value $Line
}

# Run a native command with optional extra environment variables, tee its
# output to NAME.log, and record PASS/FAIL by its exit code.
function Run-Step([string]$Name, [hashtable]$Env, [string]$Exe, [string[]]$ArgList) {
  $log = Join-Path $OutDir "$Name.log"
  Write-Host ''
  Write-Host "==> ${Name}: $Exe $($ArgList -join ' ')"
  $saved = @{}
  if ($Env) {
    foreach ($key in $Env.Keys) {
      $saved[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
      [Environment]::SetEnvironmentVariable($key, $Env[$key], 'Process')
    }
  }
  try {
    # To the host, not the pipeline: anything left on the pipeline becomes
    # part of this function's return value, and `if (Run-Step ...)` would then
    # be true for any step that printed a line.
    # Not Tee-Object: in Windows PowerShell 5.1 it writes UTF-16, which grep
    # and every reader on the other side of the first real run (2026-10-15)
    # saw as a binary file. Out-File -Encoding utf8 behaves the same in 5.1
    # and 7.
    & $Exe @ArgList 2>&1 | ForEach-Object { "$_" } |
      ForEach-Object { Write-Host $_; $_ } |
      Out-File -FilePath $log -Encoding utf8
    $rc = $LASTEXITCODE
  } finally {
    foreach ($key in $saved.Keys) {
      [Environment]::SetEnvironmentVariable($key, $saved[$key], 'Process')
    }
  }
  if ($rc -eq 0) { Say "PASS  $Name"; return $true }
  Say "FAIL  $Name (exit $rc; see $Name.log)"
  $script:Failed = $true
  return $false
}

function Skip-Step([string]$Name, [string]$Reason) { Say "SKIP  $Name ($Reason)" }

# ── header ──────────────────────────────────────────────────────────────────
$commit = $null; $dirty = $null
if (Get-Command git -ErrorAction SilentlyContinue) {
  $commit = (git rev-parse HEAD 2>$null)
  $dirty = (git status --porcelain --untracked-files=no 2>$null)
}
# The first real run printed an empty commit line: git was not on this
# shell's PATH (Git for Windows installs it on the PATH only if asked).
if (-not $commit) { $commit = 'unknown (git not on PATH, or not a git checkout)' }
$pinned = ((Get-Content rust-toolchain.toml | Select-String '^channel').Line -split '"')[1]
$rustc = (rustc -V 2>$null)
if (-not $rustc) { $rustc = 'NOT RESOLVED - rustup could not provide the pinned toolchain' }
Set-Content -Path $Summary -Value @(
  "vieww Windows hardware run $stamp",
  "commit: $commit$(if ($dirty) { ' (dirty)' })",
  "host: $env:COMPUTERNAME - $([Environment]::OSVersion.VersionString)",
  "cpu: $((Get-CimInstance Win32_Processor | Select-Object -First 1).Name)",
  "rustc: $rustc (pinned: $pinned)",
  ''
)

$hasVulkan = $false
$vulkanText = ''
if (Get-Command vulkaninfo -ErrorAction SilentlyContinue) {
  $vulkanText = (vulkaninfo --summary 2>&1 | Out-String)
  $hasVulkan = ($LASTEXITCODE -eq 0)
  Set-Content -Path (Join-Path $OutDir 'vulkaninfo.txt') -Value $vulkanText
} else {
  # No SDK: the loader may still be there (every GPU driver installs it). The
  # suites will say for themselves; assume present and let them fail loudly.
  $hasVulkan = Test-Path (Join-Path $env:SystemRoot 'System32\vulkan-1.dll')
}

if (Want 'env') {
  foreach ($gpu in Get-CimInstance Win32_VideoController) {
    Say "gpu: $($gpu.Name) (driver $($gpu.DriverVersion))"
  }
  $battery = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($battery) {
    if ($battery.BatteryStatus -eq 2) { Say 'power: on mains' }
    else { Say 'power: ON BATTERY - throttling and power saving will move every number' }
  } else { Say 'power: desktop (no battery)' }
  Say "power plan: $((powercfg /getactivescheme) -replace '^.*\((.*)\).*$','$1')"
  if ($vulkanText) {
    foreach ($m in ([regex]'(?m)^\s*deviceName\s*=\s*(.+)$').Matches($vulkanText)) {
      Say "vulkan: $($m.Groups[1].Value.Trim())"
    }
  } elseif ($hasVulkan) {
    Say 'vulkan: loader present (install the Vulkan SDK for vulkaninfo)'
  } else {
    Say 'vulkan: NO LOADER - the gpu, gallery and baseline steps will fail'
  }
  if ($rustc -notmatch " $([regex]::Escape($pinned)) ") {
    Say "      WARNING: rustc is not the pinned $pinned - numbers are for a different compiler"
  }
  Say '      energy: not measured on Windows (no package energy counter without a driver); reported as null'
}

# ── GPU (Vulkan on this PC's driver) ────────────────────────────────────────
if (Want 'gpu') {
  if ($hasVulkan) {
    Run-Step 'gpu-hal' @{ VIEWW_REQUIRE_HARDWARE = 'gpu' } 'cargo' @('test','--release','-p','vieww-hal','--features','vulkan','--','--ignored','--test-threads=1') | Out-Null
    Run-Step 'gpu-3d' @{ VIEWW_REQUIRE_HARDWARE = 'gpu' } 'cargo' @('test','--release','-p','vieww-3d','--features','vulkan','--','--ignored','--test-threads=1') | Out-Null
    Run-Step 'gpu-window' @{ VIEWW_REQUIRE_HARDWARE = 'display,gpu' } 'cargo' @('test','--release','-p','vieww-platform-winit','--test','wait_loop') | Out-Null
  } else { Skip-Step 'gpu' 'no Vulkan loader' }
}

# ── D3D12 on a hardware adapter ─────────────────────────────────────────────
if (Want 'd3d12') {
  # No VIEWW_D3D12_ALLOW_WARP: this must be the real adapter, which is the
  # whole difference from the hosted CI leg.
  Run-Step 'd3d12-hardware' @{ VIEWW_REQUIRE_HARDWARE = 'gpu'; VIEWW_D3D12_ALLOW_WARP = $null } 'cargo' @('test','--release','-p','vieww-hal','--features','d3d12','--','--ignored','--test-threads=1') | Out-Null
  Say '      d3d12: bring-up and smoke suite only - the backend does not execute a ScenePlan yet (PENDING 1.3)'
}

# ── gallery and census ──────────────────────────────────────────────────────
if (Want 'gallery') {
  $fx = Join-Path $OutDir 'fixtures'
  $cs = Join-Path $OutDir 'census'
  Run-Step 'gallery' @{} 'cargo' @('run','--release','-p','fixtures','--',$fx,'--runs','30','--receipt',(Join-Path $fx 'receipt.json'),'--shadow',(Join-Path $fx 'shadow.jsonl')) | Out-Null
  Run-Step 'census' @{} 'cargo' @('run','--release','-p','fixtures','--',$cs,'--census','--runs','30','--receipt',(Join-Path $cs 'receipt.json'),'--shadow',(Join-Path $cs 'shadow.jsonl')) | Out-Null
  $receipt = Join-Path $cs 'receipt.json'
  if (Test-Path $receipt) {
    $r = Get-Content $receipt -Raw | ConvertFrom-Json
    $gpuName = if ($r.gpu) { "$($r.gpu.name) [$($r.gpu.device_type)]" } else { 'none' }
    Say "      census: gpu=$gpuName rendered_on_gpu=$($r.outcome.rendered_on_gpu)/$($r.outcome.fixtures) parity=$($r.outcome.parity_verdict)"
  }
}

# ── baseline: a real window, launched N times per path ──────────────────────
$python = $null
foreach ($candidate in 'python','py','python3') {
  if (Get-Command $candidate -ErrorAction SilentlyContinue) { $python = $candidate; break }
}
if (Want 'baseline') {
  if (Run-Step 'baseline-build' @{} 'cargo' @('build','--release','-p','vieww-platform-winit','--example','baseline')) {
    $bin = Join-Path $Root 'target\release\examples\baseline.exe'
    $dir = Join-Path $OutDir 'baseline'
    if (-not (Test-Path $bin)) {
      Say "FAIL  baseline (built, but $bin is not there)"
      $script:Failed = $true
      $Launches = 0
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Say '      baseline start-up is warm-cache, and launch-to-first-frame includes PowerShell''s process creation (main-to-first-frame does not)'
    foreach ($mode in 'cpu','gpu') {
      $ok = $true
      for ($i = 1; $i -le $Launches; $i++) {
        $argList = @('--out', (Join-Path $dir "$mode-$i.json"))
        if ($mode -eq 'gpu') {
          $argList = @('--gpu') + $argList
          $env:VIEWW_PLANNER_SHADOW = (Join-Path $dir 'gpu-shadow.jsonl')
        } else {
          Remove-Item Env:\VIEWW_PLANNER_SHADOW -ErrorAction SilentlyContinue
        }
        # Unix nanoseconds from .NET ticks (100 ns since 0001-01-01).
        $env:VIEWW_BASELINE_LAUNCH_UNIX_NS = [string](([DateTime]::UtcNow.Ticks - 621355968000000000) * 100)
        $p = Start-Process -FilePath $bin -ArgumentList $argList -NoNewWindow -PassThru `
          -RedirectStandardOutput (Join-Path $OutDir "baseline-$mode-$i.out.log") `
          -RedirectStandardError (Join-Path $OutDir "baseline-$mode-$i.err.log")
        $null = $p.Handle  # keeps ExitCode readable after exit
        if (-not $p.WaitForExit(120000)) {
          $p.Kill()
          Add-Content (Join-Path $OutDir "baseline-$mode-$i.err.log") 'baseline: killed after 120 s'
          $ok = $false
        } elseif ($p.ExitCode -ne 0) { $ok = $false }
      }
      Remove-Item Env:\VIEWW_PLANNER_SHADOW -ErrorAction SilentlyContinue
      Remove-Item Env:\VIEWW_BASELINE_LAUNCH_UNIX_NS -ErrorAction SilentlyContinue
      if ($Launches -eq 0) { continue }
      if ($ok) { Say "PASS  baseline-$mode ($Launches launches)" }
      else { Say "FAIL  baseline-$mode (see baseline-$mode-*.err.log)"; $script:Failed = $true }
    }
    if ($python) {
      $text = & $python (Join-Path $Root 'ci\hardware\summarize_baseline.py') $dir 2>&1 | ForEach-Object { "$_" }
      Set-Content -Path (Join-Path $dir 'summary.txt') -Value $text
      foreach ($line in $text) { Say "      $line" }
    } else { Say '      (no Python: baseline/*.json are the raw reports; summarize_baseline.py was not run)' }
  }
}

# ── size of what ships ──────────────────────────────────────────────────────
if (Want 'size') {
  # `dist` (Cargo.toml): LTO, stripped, dependencies at opt-level "s".
  if (Run-Step 'size-build' @{} 'cargo' @('build','--profile','dist','-p','vieww-platform-winit','--example','baseline','--target-dir','target\size')) {
    $exe = Join-Path $Root 'target\size\dist\examples\baseline.exe'
    if (Test-Path $exe) {
      $bytes = (Get-Item $exe).Length
      Set-Content -Path (Join-Path $OutDir 'size-baseline-bytes.txt') -Value $bytes
      Say ("      windowed baseline app, stripped release: {0} bytes ({1:N1} MB)" -f $bytes, ($bytes / 1MB))
    }
  }
}

# ── animation stress, five times ────────────────────────────────────────────
if (Want 'stress') {
  $p95s = @()
  for ($i = 1; $i -le 5; $i++) {
    $dir = Join-Path $OutDir "stress-$i"
    Run-Step "stress-$i" @{} 'cargo' @('run','--release','-p','test-animation-stress','--',$dir) | Out-Null
    $metrics = Join-Path $dir 'metrics.txt'
    if (Test-Path $metrics) {
      $line = Get-Content $metrics | Where-Object { $_ -like 'render_p95_ms=*' } | Select-Object -First 1
      if ($line) { $p95s += [double]($line -split '=')[1] }
    }
  }
  if ($p95s.Count -gt 0) {
    Say "      animation stress p95 over $($p95s.Count) runs (ms): $(($p95s | Sort-Object) -join ' ')"
  }
}

# ── the on-desktop suite ────────────────────────────────────────────────────
# Git Bash, found the way Git for Windows actually installs it: only
# `git.exe` (from Git\cmd) goes on PATH by default, while bash.exe is in
# Git\bin — so "git is installed" and "bash is not on PATH" are both true on
# a normal machine, which is what the first real run hit. Never
# System32\bash.exe: that is WSL's, and it would run the suite in Linux.
function Find-GitBash {
  $candidates = @()
  $git = Get-Command git -ErrorAction SilentlyContinue
  if ($git) {
    $dir = Split-Path -Parent $git.Source           # ...\Git\cmd or ...\Git\mingw64\bin
    $candidates += (Join-Path $dir '..\bin\bash.exe')
    $candidates += (Join-Path $dir '..\..\bin\bash.exe')
  }
  $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)})
  if ($env:LOCALAPPDATA) { $roots += (Join-Path $env:LOCALAPPDATA 'Programs') }
  foreach ($root in $roots) {
    if ($root) { $candidates += (Join-Path $root 'Git\bin\bash.exe') }
  }
  $onPath = Get-Command bash -ErrorAction SilentlyContinue
  if ($onPath -and $onPath.Source -notmatch '[\\/]System32[\\/]') { $candidates += $onPath.Source }
  foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) { return (Resolve-Path $c).Path }
  }
  return $null
}

if (Want 'desktop') {
  $bash = Find-GitBash
  if ($bash) {
    Say "      desktop: using $bash"
    Run-Step 'desktop' @{} $bash @('ci/certify/desktop-suite.sh','--log',(Join-Path $OutDir 'desktop-app.log')) | Out-Null
  } else { Skip-Step 'desktop' 'Git Bash not found (looked next to git.exe and in Program Files\Git\bin)' }
}

# ── the studio's out-of-process preview ─────────────────────────────────────
if (Want 'remote') {
  Run-Step 'remote' @{} 'cargo' @('test','-p','viewwstudio','--test','remote','--','--test-threads=1') | Out-Null
}

Write-Host ''
Write-Host '================ summary ================'
Get-Content $Summary
Write-Host ''
Write-Host "results: $OutDir"
if ($script:Failed) { exit 1 } else { exit 0 }
