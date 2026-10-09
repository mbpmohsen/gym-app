# Builds everything and the installer: web UI, gym-server, face-service, then Inno Setup.
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1
# Output: installer\Output\GymApp-Setup-<version>.exe

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

function Step($name, $dir, [scriptblock]$cmd) {
    Write-Host "`n== $name" -ForegroundColor Cyan
    Push-Location $dir
    try { & $cmd; if ($LASTEXITCODE -ne 0) { throw "$name failed (exit $LASTEXITCODE)" } }
    finally { Pop-Location }
}

Step 'web UI' "$root\web" { npm run build }
Step 'gym-server' "$root\server" { cargo build --release }
Step 'face-service' "$root\face-service" { cargo build --release --bin face-service }

foreach ($f in @("$root\face-service\onnxruntime.dll", "$root\face-service\models\face_detection_yunet_2023mar.onnx", "$root\face-service\models\face_recognition_sface_2021dec.onnx")) {
    if (-not (Test-Path $f)) { throw "missing $f (see face-service\README.md)" }
}

$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found: winget install JRSoftware.InnoSetup' }

Step 'installer' $PSScriptRoot { & $iscc 'gym-app.iss' }
Write-Host "`nDone: $(Get-ChildItem "$PSScriptRoot\Output\*.exe" | Sort-Object LastWriteTime | Select-Object -Last 1)" -ForegroundColor Green
