#Requires -Version 5.1
<#
.SYNOPSIS
Recreate the Windows spatial-engine setup from GitHub after deleting the local
working copies.

.DESCRIPTION
Sources live in two repositories:

    Akramx15/A2DP-Windows-Bridge                       app fork, public
    Akramx15/xm6-spatial-linux                         engine, private

Everything that cannot be produced by a compiler alone (the prebuilt Sony
stage executables, the Upmixer stage payload, the win_compat shim) is stored in
the engine repository under engine/win-port, so this script only needs a clone,
Python and the network.

    powershell -ExecutionPolicy Bypass -File tools/xm6/restore.ps1

Steps:
  1. clones both repositories under -Root when they are missing,
  2. copies the prebuilt stages to %APPDATA%\A2DPWB\stages,
  3. recreates the pinned Upmixer runtime
     (numpy 2.3.5 / scipy 1.16.3 / soundfile 0.13.1) in
     %APPDATA%\A2DPWB\runtime\upmixer-venv,
  4. points the Upmixer stage command at that interpreter,
  5. re-points every profile in %APPDATA%\A2DPWB\profiles.json at the new
     stage directory,
  6. re-verifies all five stages byte-for-byte.

The application binaries themselves are not restored here: rebuild them with
tools/xm6/apply-upstream.ps1 -SkipInstall, or install a release.

.PARAMETER Root
Directory that holds (or will hold) both clones. Default: the parent of this
repository.

.PARAMETER Python
Base interpreter used to create the Upmixer runtime. Default: python on PATH.

.PARAMETER SkipClone
Use the existing clones instead of cloning.

.PARAMETER EngineDir
Existing engine clone to use (implies -SkipClone for that side).
Default: <Root>\xm6-spatial-linux.

.PARAMETER ForceRuntime
Recreate the Upmixer runtime even if it already exists.
#>
[CmdletBinding()]
param(
    [string]$Root,
    [string]$EngineDir,
    [string]$Python = 'python',
    [switch]$SkipClone,
    [switch]$ForceRuntime
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Fail($text) { Write-Host "error: $text" -ForegroundColor Red; exit 1 }

$repo = (git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
if (-not $Root) { $Root = Split-Path $repo -Parent }
$forkUrl = 'https://github.com/Akramx15/A2DP-Windows-Bridge.git'
$engineUrl = 'https://github.com/Akramx15/xm6-spatial-linux.git'
$forkDir = Join-Path $Root 'A2DP-Windows-Bridge'
if (-not $EngineDir) { $EngineDir = Join-Path $Root 'xm6-spatial-linux' }
$engineDir = $EngineDir
$appData = Join-Path $env:APPDATA 'A2DPWB'
$stagesDir = Join-Path $appData 'stages'
$runtimeDir = Join-Path $appData 'runtime\upmixer-venv'
$upmixerPython = Join-Path $runtimeDir 'Scripts\python.exe'
$pins = @('numpy==2.3.5', 'scipy==1.16.3', 'soundfile==0.13.1')

if (-not $SkipClone) {
    Step "cloning into $Root"
    if (-not (Test-Path $engineDir)) {
        # The engine repository is private: gh supplies the credentials.
        if (Get-Command gh -ErrorAction SilentlyContinue) { gh repo clone Akramx15/xm6-spatial-linux $engineDir }
        else { git clone $engineUrl $engineDir }
        if ($LASTEXITCODE) { Fail "cloning $engineUrl failed (the repository is private: run gh auth login)" }
    }
    else { Write-Host "  engine clone already present" -ForegroundColor Yellow }
    if (-not (Test-Path (Join-Path $forkDir '.git'))) {
        git clone $forkUrl $forkDir
        if ($LASTEXITCODE) { Fail "cloning $forkUrl failed" }
        git -C $forkDir remote add upstream https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
        git -C $forkDir fetch upstream --tags
        git -C $forkDir checkout feat/xm6-360-spatial-engine-v2.0.2
        if ($LASTEXITCODE) { Fail 'checking out the feature branch failed' }
    }
    else { Write-Host "  app clone already present" -ForegroundColor Yellow }

    Step 'initialising the codec submodules'
    # libldac, libopenaptx, btstack, fdk-aac, lhdcv5-enc and opus are submodules:
    # without them the Rust build stops at the first missing include.
    git -C $forkDir submodule update --init --recursive
    if ($LASTEXITCODE) { Fail 'git submodule update failed' }
}

$prebuilt = Join-Path $engineDir 'engine\win-port\prebuilt\stages'
if (-not (Test-Path $prebuilt)) { Fail "no prebuilt stages at $prebuilt (is the engine clone up to date?)" }

Step "creating the Upmixer runtime in $runtimeDir"
if ($ForceRuntime -or -not (Test-Path $upmixerPython)) {
    if (Test-Path $runtimeDir) { Remove-Item $runtimeDir -Recurse -Force }
    & $Python -m venv $runtimeDir
    if ($LASTEXITCODE) { Fail "creating the venv with $Python failed" }
    & $upmixerPython -m pip install --disable-pip-version-check --quiet @pins
    if ($LASTEXITCODE) { Fail 'installing the pinned packages failed (network?)' }
}
else { Write-Host '  runtime already present (use -ForceRuntime to recreate)' -ForegroundColor Yellow }
& $upmixerPython -c "import numpy, scipy, soundfile; print('  runtime ok', numpy.__version__, scipy.__version__, soundfile.__version__)"
if ($LASTEXITCODE) { Fail 'the Upmixer runtime cannot import its dependencies' }

Step "installing the stages into $stagesDir"
New-Item -ItemType Directory -Force $stagesDir | Out-Null
Copy-Item (Join-Path $prebuilt '*') $stagesDir -Recurse -Force
$marker = '__UPMIXER_PYTHON__'
$manifestPath = Join-Path $stagesDir 'experimental-stft\STAGE-MANIFEST.json'
$manifest = [IO.File]::ReadAllText($manifestPath)
if ($manifest -notmatch [regex]::Escape($marker)) { Fail "the Upmixer manifest no longer mentions $marker" }
$manifest = $manifest.Replace($marker, $upmixerPython.Replace('\', '\\'))
[IO.File]::WriteAllText($manifestPath, $manifest, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "  $((Get-ChildItem $stagesDir -Directory).Count) stages"

Step 're-pointing the profiles'
$profilesPath = Join-Path $appData 'profiles.json'
if (Test-Path $profilesPath) {
    if (Get-Process a2dpwb, a2dpwb-core -ErrorAction SilentlyContinue) {
        Fail 'the application is running; close it and run this script again'
    }
    $raw = [IO.File]::ReadAllText($profilesPath)
    Copy-Item $profilesPath "$profilesPath.before-restore" -Force
    $registry = $stagesDir.Replace('\', '\\')
    $raw = [regex]::Replace($raw, '("spatial_engine_registry"\s*:\s*")[^"]*(")', { param($m) $m.Groups[1].Value + $registry + $m.Groups[2].Value })
    $raw = [regex]::Replace($raw, '("spatial_engine_path"\s*:\s*")[^"]*(")', { param($m) $m.Groups[1].Value + $registry + '\\\\experimental-stft\\\\libxm6_experimental.dll' + $m.Groups[2].Value })
    [IO.File]::WriteAllText($profilesPath, $raw, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "  profiles now point at $stagesDir"
}
else { Write-Host '  no profiles yet: import one in the app' -ForegroundColor Yellow }

Step 'verifying all five stages byte-for-byte'
python (Join-Path $engineDir 'engine\win-port\qa\verify_win_pcm.py')
if ($LASTEXITCODE) { Fail 'a stage does not reproduce its recorded PCM' }

Write-Host @"

Stages and runtime are in place. Next:

    cd $forkDir
    git checkout feat/xm6-360-spatial-engine-v2.0.2
    powershell -ExecutionPolicy Bypass -File tools/xm6/apply-upstream.ps1 -SkipInstall

then start A2DP Windows Bridge and pick the xm6 profile.
"@ -ForegroundColor Green
