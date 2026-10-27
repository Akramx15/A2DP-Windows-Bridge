#Requires -Version 5.1
<#
.SYNOPSIS
Bring the xm6 spatial-engine feature onto a new upstream release.

.DESCRIPTION
One command per upstream update:

    powershell -ExecutionPolicy Bypass -File tools/xm6/apply-upstream.ps1

What it does, in order:
  1. fetches upstream tags,
  2. resolves the target tag (newest v* unless -Tag is given),
  3. refuses to run on a dirty tree,
  4. creates feat/xm6-360-spatial-engine-<tag> from that tag,
  5. merges the previous feature branch into it (conflicts are left in place
     for you to resolve, nothing is thrown away),
  6. checks the language files after the merge,
  7. builds the UI and the three executables in release profile,
  8. copies them into the installed app, keeping *.bak copies of the previous
     binaries,
  9. optionally re-verifies the spatial stages byte-for-byte.

Nothing is pushed. Review the merge, then push the branch yourself.

.PARAMETER Tag
Upstream tag to move onto, e.g. v2.0.2-beta.6. Default: newest upstream v* tag.

.PARAMETER From
Feature branch to merge in. Default: the branch that was checked out.

.PARAMETER SkipBuild
Only do the git work (fetch, branch, merge, language check).

.PARAMETER SkipInstall
Build but do not touch the installed application.

.PARAMETER Test
Also run the repository checks before building (clippy, cargo test, tsc,
vitest, check_lang). Slower but catches a bad merge before it reaches you.

.PARAMETER EngineDir
Engine clone used for the final byte-exact stage verification.
Default: the sibling of this repository, i.e. <parent>\xm6-spatial-linux.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File tools/xm6/apply-upstream.ps1 -Tag v2.0.2-beta.6
#>
[CmdletBinding()]
param(
    [string]$Tag,
    [string]$From,
    [string]$EngineDir,
    [switch]$SkipBuild,
    [switch]$SkipInstall,
    [switch]$Test
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
Set-Location $repo

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Fail($text) { Write-Host "error: $text" -ForegroundColor Red; exit 1 }

$dirty = git status --porcelain --untracked-files=no
if ($dirty) {
    Fail 'the working tree has uncommitted changes; commit or stash them first'
}

if (-not $From) { $From = (git rev-parse --abbrev-ref HEAD).Trim() }
if ($From -eq 'HEAD') { Fail 'detached HEAD: pass -From <feature-branch>' }

Step 'fetching upstream'
git fetch upstream --tags --prune
if ($LASTEXITCODE) { Fail 'git fetch upstream failed' }

if (-not $Tag) {
    $Tag = (git tag --list 'v*' --sort=-v:refname | Select-Object -First 1)
    if (-not $Tag) { Fail 'no upstream tags found' }
}
git rev-parse --verify --quiet "refs/tags/$Tag" | Out-Null
if ($LASTEXITCODE) { Fail "unknown tag $Tag" }

$branch = "feat/xm6-360-spatial-engine-$Tag"
Step "creating $branch from $Tag (merging $From)"
git rev-parse --verify --quiet "refs/heads/$branch" | Out-Null
if ($LASTEXITCODE -eq 0) {
    Write-Host "branch $branch already exists; checking it out as is" -ForegroundColor Yellow
    git checkout $branch
    if ($LASTEXITCODE) { Fail "cannot check out $branch" }
}
else {
    git checkout -b $branch "$Tag"
    if ($LASTEXITCODE) { Fail "cannot create $branch from $Tag" }

    git merge --no-ff --no-commit $From
    if ($LASTEXITCODE) {
        $conflicts = git diff --name-only --diff-filter=U
        Write-Host "`nmerge stopped on conflicts:" -ForegroundColor Yellow
        $conflicts | ForEach-Object { Write-Host "  $_" }
        Write-Host @"

Resolve them, then:

    git add -A
    git commit
    powershell -ExecutionPolicy Bypass -File tools/xm6/apply-upstream.ps1 -Tag $Tag -From $From -SkipBuild

Nothing was pushed and no file was thrown away.
"@
        exit 2
    }
    $message = @"
Move the spatial engine onto $Tag

Merged $From into the $Tag release. The engine sources, stage payloads and
win_compat shim are unchanged; only the app-side wiring was re-merged.
"@
    if (Test-Path (Join-Path $repo '.git/MERGE_HEAD')) {
        git commit -q -m $message
        if ($LASTEXITCODE) { Fail 'git commit failed' }
    }
    else {
        Write-Host "  $From is already contained in $Tag; nothing to merge" -ForegroundColor Yellow
    }
}

Step 'checking the language files'
python tools/check_lang.py
if ($LASTEXITCODE) { Fail 'tools/check_lang.py failed: a new label is missing a translation' }

if ($SkipBuild) {
    Write-Host "`ngit work done on $branch; build skipped by -SkipBuild" -ForegroundColor Green
    exit 0
}

if ($Test) {
    Step 'repository checks'
    cargo fmt --check
    if ($LASTEXITCODE) { Fail 'cargo fmt --check failed' }
    cargo clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE) { Fail 'clippy failed' }
    cargo test --workspace
    if ($LASTEXITCODE) { Fail 'cargo test failed' }
    pnpm --dir app/src/ui exec tsc --noEmit
    if ($LASTEXITCODE) { Fail 'tsc failed' }
    pnpm --dir app/src/ui test
    if ($LASTEXITCODE) { Fail 'vitest failed' }
}

Step 'building the UI'
if (-not (Test-Path 'app/src/ui/node_modules')) {
    pnpm --dir app/src/ui install --frozen-lockfile
    if ($LASTEXITCODE) { Fail 'pnpm install failed' }
}
pnpm --dir app/src/ui build
if ($LASTEXITCODE) { Fail 'UI build failed' }

Step 'building the executables'
cargo build --release -p a2dpwb-core-exe -p a2dpwb-driver -p a2dpwb-ui
if ($LASTEXITCODE) { Fail 'cargo build failed' }

$artifacts = @('a2dpwb.exe', 'a2dpwb-core.exe', 'a2dpwb-driver.exe')
foreach ($name in $artifacts) {
    if (-not (Test-Path "target/release/$name")) { Fail "missing target/release/$name" }
}

if ($SkipInstall) {
    Write-Host "`nbuilt target/release/*.exe; install skipped by -SkipInstall" -ForegroundColor Green
    exit 0
}

$install = Join-Path $env:LOCALAPPDATA 'A2DP Windows Bridge'
if (-not (Test-Path $install)) {
    Write-Host "`nno installation at $install; run the installer, then copy target/release/*.exe there" -ForegroundColor Yellow
    exit 0
}

Step "installing into $install"
if (Get-Process a2dpwb, a2dpwb-core -ErrorAction SilentlyContinue) {
    Write-Host 'closing the running application' -ForegroundColor Yellow
    Get-Process a2dpwb, a2dpwb-core -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
}
foreach ($name in $artifacts) {
    $target = Join-Path $install $name
    if (Test-Path $target) { Copy-Item $target "$target.bak" -Force }
    Copy-Item "target/release/$name" $target -Force
    Write-Host "  $name"
}

Step 're-verifying the spatial stages'
if (-not $EngineDir) { $EngineDir = Join-Path (Split-Path $repo -Parent) 'xm6-spatial-linux' }
$verifier = Join-Path $EngineDir 'engine/win-port/qa/verify_win_pcm.py'
if (Test-Path $verifier) {
    python $verifier
    if ($LASTEXITCODE) { Write-Host 'stage PCM differs: investigate before listening' -ForegroundColor Yellow }
}
else {
    Write-Host "skipped: $verifier not found (pass -EngineDir or clone xm6-spatial-linux beside this repository)" -ForegroundColor Yellow
}

Write-Host @"

Done. $branch is ready but not pushed:

    git push -u origin $branch

Then start the app and pick the xm6 profile.
"@ -ForegroundColor Green
