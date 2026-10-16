# SPDX-License-Identifier: MPL-2.0
#
# One-shot Windows setup + build for wjson. No Visual Studio / MSVC needed.
#
#   Right-click > "Run with PowerShell", or:
#   powershell -NoExit -ExecutionPolicy Bypass -File .\setup-windows.ps1
#
# Installs what is missing, builds wjlink, then runs `wavec run build.wave`
# with wjlink as the linker (build.wave itself is a Wave program):
#   1. Rust (rustup). Its rust-lld is the linker; wjson's own wjlink (chain/,
#      built first by build.wave) generates the import libraries from the
#      system DLLs, so neither MSVC nor the Windows SDK is used.
#   2. clang (LLVM) for the C++ half, compiled freestanding (no SDK headers).
#   3. Wave compiler (official installer, per-user, no admin).
# Everything is logged to setup-log.txt; the build output goes to run-log.txt.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Start-Transcript -Path (Join-Path $root 'setup-log.txt') -Force | Out-Null

function Have($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

$llvmBin = Join-Path $env:ProgramFiles 'LLVM\bin'

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = "$machine;$user;$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\Wave\bin;$llvmBin"
}

function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }

function Winget-Install($id, $extra) {
    if (-not (Have 'winget')) {
        throw "winget not found. Install 'App Installer' from the Microsoft Store, then run this script again."
    }
    $wingetArgs = @('install', '--id', $id, '-e', '--accept-source-agreements', '--accept-package-agreements') + $extra
    Write-Host "    winget $($wingetArgs -join ' ')"
    & winget @wingetArgs
    # 0 = installed, -1978335189 = already installed / no newer version
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
        throw "winget install $id failed (exit code $LASTEXITCODE)"
    }
}

try {
    Refresh-Path

    # 1. Rust ---------------------------------------------------------------------
    Step 'Rust'
    if (-not (Have 'cargo')) {
        Winget-Install 'Rustlang.Rustup' @()
        Refresh-Path
        if (-not (Have 'cargo')) { throw 'cargo still not found after installing rustup.' }
    }
    # Cargo.toml asks for Rust 1.99 or newer; bring an older rustup install up to date.
    # No 2>&1 here: rustup reports progress on stderr, and Windows PowerShell 5.1
    # turns redirected native stderr into a terminating error under 'Stop'. A
    # failed update is not fatal; cargo itself says if the version is too old.
    if (Have 'rustup') {
        & rustup update stable --no-self-update
        if ($LASTEXITCODE -ne 0) { Write-Host '    note: rustup update failed; continuing with the installed Rust' }
    }
    & cargo --version 2>&1 | Out-String | Write-Host
    $sysroot = (& rustc --print sysroot).Trim()
    $lld = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe'
    if (-not (Test-Path $lld)) { throw "rust-lld not found at $lld (is the x86_64-pc-windows-msvc toolchain the default?)" }
    Write-Host "    linker: $lld"

    # 2. clang --------------------------------------------------------------------
    Step 'clang (LLVM) for the C++ sources'
    if (-not (Have 'clang++')) {
        Write-Host '    not found - installing LLVM (a UAC prompt may appear; click Yes).'
        Winget-Install 'LLVM.LLVM' @('--silent')
        Refresh-Path
        if (-not (Have 'clang++')) { throw 'clang++ still not found after installing LLVM.' }
    }
    & clang++ --version 2>&1 | Select-Object -First 1 | Out-String | Write-Host

    # 3. Wave ---------------------------------------------------------------------
    Step 'Wave compiler'
    if (-not (Have 'wavec')) {
        $installer = Join-Path $env:TEMP 'wave-install.ps1'
        Invoke-RestMethod 'https://wave-lang.dev/install.ps1' -OutFile $installer
        # The installer runs with ExecutionPolicy Bypass, so refuse anything but
        # the exact script that was reviewed. (It installs Wave v0.2.1-pre-beta
        # and checks the GitHub-published SHA-256 of what it downloads.) If the
        # hash changes, read the new script, then update the value below.
        $expectedInstaller = '7d8c9f28ecc9ca05bafec8a036b0efac1694644484d48368bd28d3943bd477aa'
        $actualInstaller = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
        if ($actualInstaller -ine $expectedInstaller) {
            Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
            throw "wave-lang.dev/install.ps1 does not match the pinned SHA-256 (got $actualInstaller). Review the new installer, then update `$expectedInstaller in setup-windows.ps1, or install Wave by hand from https://github.com/wavefnd/Wave/releases."
        }
        & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -WithoutVex
        if ($LASTEXITCODE -ne 0) { throw "Wave installer failed (exit code $LASTEXITCODE)" }
        $env:Path = "$env:Path;$env:LOCALAPPDATA\Wave\bin"
        if (-not (Have 'wavec')) { throw 'wavec still not found after installation.' }
    }
    # The Windows installer puts std next to wavec.exe, and wavec does not look
    # there on its own (the installer's own smoke test passes --std-root too).
    $wavecExe = (Get-Command wavec).Source
    $std = Join-Path (Split-Path -Parent $wavecExe) 'std'
    if (-not $env:WAVE_STD -and (Test-Path (Join-Path $std 'manifest.json'))) { $env:WAVE_STD = $std }
    Write-Host "    wavec: $wavecExe"
    Write-Host "    std  : $env:WAVE_STD"
    & wavec --version 2>&1 | Out-String | Write-Host

    # Make sure nothing from an installed Visual Studio leaks in: no MSVC
    # library or include paths, no developer-shell variables.
    foreach ($v in 'LIB', 'LIBPATH', 'INCLUDE', 'VCINSTALLDIR', 'VSINSTALLDIR', 'WindowsSdkDir') {
        Remove-Item "Env:$v" -ErrorAction SilentlyContinue
    }
    if (Have 'cl') { Write-Host '    note: cl.exe is on PATH but is not used' }

    # 4. wjlink -------------------------------------------------------------------
    # build.wave is itself a Wave program that wavec must link, so the linker
    # comes first: wjlink links with rust-lld alone (no_std + raw-dylib), then
    # generates the import libraries it needs from the system DLLs.
    Step 'wjlink: MSVC-free linker (Rust)'
    & cargo build --release --quiet --manifest-path chain/Cargo.toml --target-dir build/chain `
        --config "target.x86_64-pc-windows-msvc.linker='rust-lld'" `
        --config "target.x86_64-pc-windows-msvc.rustflags=['-Clink-arg=/NODEFAULTLIB']"
    if ($LASTEXITCODE -ne 0) { throw "building wjlink failed (exit code $LASTEXITCODE)" }
    $wjlink = Join-Path $root 'build\chain\release\wjlink.exe'
    & $wjlink init --lld $lld | Select-Object -Last 1 | Write-Host
    if ($LASTEXITCODE -ne 0) { throw "wjlink init failed (exit code $LASTEXITCODE)" }

    # 5. Build and test -----------------------------------------------------------
    Step 'wavec run build.wave  (output also saved to run-log.txt)'
    $ErrorActionPreference = 'Continue'
    $stdArg = ''
    if ($env:WAVE_STD) { $stdArg = "--std-root=`"$env:WAVE_STD`"" }
    $linkArgs = '-Clinker=build\chain\release\wjlink.exe -L build\chain\release\lib'
    Write-Host "    wavec $stdArg $linkArgs run build.wave"
    & cmd /c "wavec $stdArg $linkArgs run build.wave > run-log.txt 2>&1"
    $code = $LASTEXITCODE
    Get-Content -Path (Join-Path $root 'run-log.txt') -Encoding UTF8
    if ($code -eq 0) {
        Write-Host '==> done: all tests passed' -ForegroundColor Green
    } else {
        Write-Host "==> build.wave exited with code $code (see run-log.txt)" -ForegroundColor Red
    }
}
catch {
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    $code = 1
}
finally {
    Stop-Transcript | Out-Null
}
exit $code
