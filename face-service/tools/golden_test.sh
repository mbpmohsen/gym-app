#!/usr/bin/env bash
# Golden test: Rust pipeline vs OpenCV (Python) on every PNG in testdata/.
# usage: tools/golden_test.sh <onnxruntime lib>
# needs: pip install opencv-python numpy   (OpenCV >= 4.8)
set -euo pipefail
LIB="$1"
cargo build --release --bin face_dump
EXE=target/release/face_dump
[ -f "$EXE.exe" ] && EXE="$EXE.exe"
OUT=$(mktemp -d)
status=0
for img in testdata/*.png; do
  n=$(basename "$img" .png)
  echo "== $n"
  "$EXE" "$LIB" models "$img" > "$OUT/$n.rust.json"
  python tools/golden.py models "$img" > "$OUT/$n.cv.json" 2>/dev/null
  python tools/check_golden.py "$OUT/$n.rust.json" "$OUT/$n.cv.json" || status=1
done
exit $status
