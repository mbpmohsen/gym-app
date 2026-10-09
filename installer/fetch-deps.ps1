# Downloads what the build needs but git doesn't hold: ONNX Runtime 1.22 (Windows x64)
# and the two face models. Skips anything already present.
#   powershell -ExecutionPolicy Bypass -File installer/fetch-deps.ps1

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with the progress bar
$fs = Join-Path (Split-Path $PSScriptRoot -Parent) 'face-service'

$ortVersion = '1.22.0'
if (-not (Test-Path "$fs\onnxruntime.dll")) {
    Write-Host "downloading ONNX Runtime $ortVersion"
    $zip = Join-Path $env:TEMP "ort-$ortVersion.zip"
    Invoke-WebRequest "https://github.com/microsoft/onnxruntime/releases/download/v$ortVersion/onnxruntime-win-x64-$ortVersion.zip" -OutFile $zip
    $dir = Join-Path $env:TEMP "ort-$ortVersion"
    Expand-Archive $zip -DestinationPath $dir -Force
    $lib = Get-ChildItem $dir -Recurse -Directory -Filter lib | Select-Object -First 1
    Copy-Item "$($lib.FullName)\onnxruntime.dll" "$fs\onnxruntime.dll"
    New-Item -ItemType Directory -Force "$fs\lib" | Out-Null
    Copy-Item "$($lib.FullName)\onnxruntime_providers_shared.dll" "$fs\lib\" -ErrorAction SilentlyContinue
}

$zoo = 'https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models'
New-Item -ItemType Directory -Force "$fs\models" | Out-Null
foreach ($m in @('face_detection_yunet/face_detection_yunet_2023mar.onnx', 'face_recognition_sface/face_recognition_sface_2021dec.onnx')) {
    $out = Join-Path "$fs\models" (Split-Path $m -Leaf)
    if (-not (Test-Path $out)) {
        Write-Host "downloading $(Split-Path $m -Leaf)"
        Invoke-WebRequest "$zoo/$m" -OutFile $out
    }
}
Write-Host 'dependencies ready'
