# tiny_os QEMU integration tests
#
# Boots the kernel on QEMU raspi4b, captures serial output, and verifies
# expected patterns appear. Exit code 0 = all pass, 1 = failure.
#
# Usage:
#   pwsh tests/qemu/run_tests.ps1
#   make test-qemu        (via Makefile)

param(
    [int]$TimeoutSeconds = 15,
    [string]$QemuPath = "qemu-system-aarch64"
)

$ErrorActionPreference = "Stop"

# Resolve paths
$ProjectRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
$KernelElf = Join-Path $ProjectRoot "target\aarch64-unknown-none\release\kernel"

Write-Host "=== tiny_os QEMU Integration Tests ===" -ForegroundColor Cyan
Write-Host ""

# Step 1: Build
Write-Host "[build] Building kernel for QEMU..." -ForegroundColor Yellow
$buildResult = & cargo build -p kernel --release --no-default-features --features kernel/bsp-qemu --manifest-path "$ProjectRoot\Cargo.toml" 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "[FAIL] Build failed:" -ForegroundColor Red
    $buildResult | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "[build] OK" -ForegroundColor Green

if (-not (Test-Path $KernelElf)) {
    Write-Host "[FAIL] Kernel ELF not found at $KernelElf" -ForegroundColor Red
    exit 1
}

# Step 2: Boot QEMU and capture output
Write-Host "[qemu] Booting with ${TimeoutSeconds}s timeout..." -ForegroundColor Yellow

$qemuArgs = "-M raspi4b -serial stdio -display none -no-reboot -smp 4 -kernel `"$KernelElf`""

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $QemuPath
$psi.Arguments = $qemuArgs
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true

$process = [System.Diagnostics.Process]::Start($psi)
$outputTask = $process.StandardOutput.ReadToEndAsync()
$errorTask = $process.StandardError.ReadToEndAsync()

if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
    $process.Kill() | Out-Null
    $process.WaitForExit(2000) | Out-Null
}

$output = $outputTask.Result
$process.Dispose()

# Save output for debugging
$outputFile = Join-Path $ProjectRoot "tests\qemu\last_output.txt"
$output | Out-File -FilePath $outputFile -Encoding utf8

$lines = $output -split "`n"
Write-Host "[qemu] Captured $($lines.Count) lines of output" -ForegroundColor Yellow

# Step 3: Check expected patterns
$tests = @(
    @{ Name = "Kernel banner";          Pattern = "tiny_os" }
    @{ Name = "UART init";              Pattern = "AArch64 EL1|uart|UART" }
    @{ Name = "MMU enabled";            Pattern = "mmu|MMU" }
    @{ Name = "Timer running";          Pattern = "timer|Timer" }
    @{ Name = "Scheduler started";      Pattern = "sched|scheduler" }
    @{ Name = "SMP core 1";            Pattern = "core 1 online|core 1:" }
    @{ Name = "SMP core 2";            Pattern = "core 2 online|core 2:" }
    @{ Name = "SMP core 3";            Pattern = "core 3 online|core 3:" }
    @{ Name = "Network loopback";       Pattern = "loopback|net:" }
    @{ Name = "Filesystem mounted";     Pattern = "fat32|ramdisk|FAT32|mount" }
    @{ Name = "User task at EL0";       Pattern = "\[user\]|EL0|user task" }
    @{ Name = "Shell prompt";           Pattern = "tiny_os>" }
    @{ Name = "No kernel panic";        Pattern = "!panic|!PANIC" }
)

$passed = 0
$failed = 0
$total = 0

Write-Host ""
Write-Host "--- Test Results ---" -ForegroundColor Cyan

foreach ($test in $tests) {
    $total++
    $name = $test.Name
    $pattern = $test.Pattern

    # Special handling for negative patterns (must NOT appear)
    if ($pattern.StartsWith("!")) {
        $realPattern = $pattern.Substring(1)
        $match = $output | Select-String -Pattern $realPattern -CaseSensitive:$false
        if ($match) {
            Write-Host "  [FAIL] $name (found: '$realPattern')" -ForegroundColor Red
            $failed++
        } else {
            Write-Host "  [PASS] $name" -ForegroundColor Green
            $passed++
        }
    } else {
        $match = $output | Select-String -Pattern $pattern -CaseSensitive:$false
        if ($match) {
            Write-Host "  [PASS] $name" -ForegroundColor Green
            $passed++
        } else {
            Write-Host "  [FAIL] $name (pattern: '$pattern' not found)" -ForegroundColor Red
            $failed++
        }
    }
}

Write-Host ""
Write-Host "--- Summary ---" -ForegroundColor Cyan
Write-Host "  Passed: $passed / $total"

if ($failed -gt 0) {
    Write-Host "  Failed: $failed" -ForegroundColor Red
    Write-Host ""
    Write-Host "Output saved to: $outputFile" -ForegroundColor Yellow
    Write-Host "First 30 lines of output:" -ForegroundColor Yellow
    $lines | Select-Object -First 30 | ForEach-Object { Write-Host "  $_" }
    exit 1
} else {
    Write-Host "  All tests passed!" -ForegroundColor Green
    exit 0
}
