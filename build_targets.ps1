# Build vtpaul for every shipping target from this Windows box, and collect
# the results into one folder you can drag anywhere.
#
#     powershell -ExecutionPolicy Bypass -File build_targets.ps1
#     powershell -ExecutionPolicy Bypass -File build_targets.ps1 -Only linux-arm64
#     powershell -ExecutionPolicy Bypass -File build_targets.ps1 -Dist D:\drop
#
# Nothing here needs a cross toolchain. The Linux targets are musl, which Rust
# links with its own bundled rust-lld and its own libc, so each one comes out
# a single static binary with no runtime dependency on the target at all --
# no glibc version matching, no apt packages. vtpaul is zero-dependency
# (Cargo.toml has no external crates), so there's nothing else to vendor.
#
# Run it from PowerShell, not cmd.

param(
    [string]$Only = "",
    [string]$Dist = "dist",
    [switch]$NoDist
)

$ErrorActionPreference = "Stop"

# n     short name, also the -Only key and the dist filename suffix
# t     rust target triple; empty means the host, built with no --target
# files artifact name -> dist name, relative to the target's release directory
$targets = @(
    @{ n = "linux-amd64"; t = "x86_64-unknown-linux-musl";
       note = "64-bit Linux PC, server, Docker, WSL";
       args = @(); files = @{ "vtpaul" = "vtpaul-linux-amd64" } },
    @{ n = "linux-arm64"; t = "aarch64-unknown-linux-musl";
       note = "64-bit Raspberry Pi OS: Pi 3/4/5, Zero 2 W";
       args = @(); files = @{ "vtpaul" = "vtpaul-linux-arm64" } },
    @{ n = "linux-armv7"; t = "armv7-unknown-linux-musleabihf";
       note = "32-bit Raspberry Pi OS on Pi 2/3/4";
       args = @(); files = @{ "vtpaul" = "vtpaul-linux-armv7" } },
    @{ n = "linux-armv6"; t = "arm-unknown-linux-musleabihf";
       note = "Pi 1 and Pi Zero / Zero W (ARMv6)";
       args = @(); files = @{ "vtpaul" = "vtpaul-linux-armv6" } },
    @{ n = "windows-amd64"; t = "";
       note = "this machine";
       args = @(); files = @{ "vtpaul.exe" = "vtpaul-windows-amd64.exe" } }
)

# rust-lld links the musl targets itself; without this cargo looks for a cross
# gcc that is not on a Windows box.
$env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "rust-lld"
$env:CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "rust-lld"
$env:CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER = "rust-lld"
$env:CARGO_TARGET_ARM_UNKNOWN_LINUX_MUSLEABIHF_LINKER = "rust-lld"

# Reproducible, path-free binaries: rustc points std's panic locations at the
# local rust-src copy (under the user profile) and cargo passes this crate's
# absolute path for some sources, so map both to fixed names. /Brepro makes the
# MSVC linker write a content hash instead of a timestamp and a random PDB GUID.
$sysroot = (& rustc --print sysroot).Trim()
$crate = (Resolve-Path $PSScriptRoot).Path
$remap = "--remap-path-prefix=$sysroot=/rustc-sysroot --remap-path-prefix=$crate=vtpaul --remap-path-prefix=$($env:USERPROFILE)=~"

$installed = (& rustup target list --installed) -split "`r?`n"

$built = @()
foreach ($x in $targets) {
    if ($Only -and $x.n -ne $Only) { continue }

    if ($x.t -and $installed -notcontains $x.t) {
        Write-Host "installing rust target $($x.t)" -ForegroundColor DarkGray
        & rustup target add $x.t
        if ($LASTEXITCODE -ne 0) { throw "rustup target add $($x.t) failed" }
    }

    Write-Host ""
    Write-Host "=== $($x.n): $(if ($x.t) { $x.t } else { 'host' }) " -NoNewline
    Write-Host "($($x.note))" -ForegroundColor DarkGray

    $env:RUSTFLAGS = if ($x.t) { $remap } else { "$remap -C link-arg=/Brepro" }
    $argv = @("build", "--release", "--bin", "vtpaul")
    if ($x.t) { $argv += @("--target", $x.t) }
    $argv += $x.args
    & cargo @argv
    if ($LASTEXITCODE -ne 0) { throw "$($x.n) failed" }

    $rel = if ($x.t) { "target/$($x.t)/release" } else { "target/release" }
    foreach ($src in $x.files.Keys) {
        $p = Join-Path $rel $src
        if (-not (Test-Path $p)) {
            Write-Host "  missing $p" -ForegroundColor Yellow
            continue
        }
        $built += [pscustomobject]@{
            Target = $x.n
            Source = (Resolve-Path $p).Path
            Name   = $x.files[$src]
            KB     = [math]::Round((Get-Item $p).Length / 1KB)
        }
    }
}


Write-Host ""
$built | Format-Table Target, Name, KB -AutoSize

if ($NoDist) { return }

# ---- collect ----------------------------------------------------------------
# One folder holding every artifact under a name that says what it runs on, so
# it can be dragged somewhere as a unit.

if (-not (Test-Path $Dist)) { New-Item -ItemType Directory -Path $Dist | Out-Null }
$distFull = (Resolve-Path $Dist).Path

foreach ($b in $built) {
    $dest = Join-Path $distFull $b.Name
    $parent = Split-Path $dest -Parent
    if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Path $parent | Out-Null }
    Copy-Item $b.Source $dest -Force
}

Get-ChildItem $distFull | Select-Object Name, Length | Format-Table -AutoSize
Write-Host "collected into $distFull" -ForegroundColor Green
