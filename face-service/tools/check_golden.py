"""Compares face_dump (Rust) output against golden.py (OpenCV) output.

usage: python tools/check_golden.py rust.json opencv.json
Exit code 1 if anything is outside tolerance.
"""
import json
import sys

import numpy as np

BOX_TOL_PX = 1.0       # OpenCV rounds boxes to ints internally in NMS; sub-pixel otherwise
LM_TOL_PX = 0.5
SCORE_TOL = 1e-3
EMB_COS_MIN = 0.995    # warpAffine uses fixed-point bilinear, so pixels differ by ±1

rust, cv = (json.load(open(p)) for p in sys.argv[1:3])
ok = True

def fail(msg):
    global ok
    ok = False
    print("FAIL", msg)

if len(rust["faces"]) != len(cv["faces"]):
    fail(f"face count rust={len(rust['faces'])} opencv={len(cv['faces'])}")

for i, (r, c) in enumerate(zip(rust["faces"], sorted(cv["faces"], key=lambda f: -f["score"]))):
    db = np.abs(np.array(r["bbox"]) - np.array(c["bbox"])).max()
    dl = np.abs(np.array(r["landmarks"]) - np.array(c["landmarks"])).max()
    ds = abs(r["score"] - c["score"])
    print(f"face {i}: bbox Δ{db:.3f}px landmarks Δ{dl:.3f}px score Δ{ds:.5f}")
    if db > BOX_TOL_PX: fail(f"face {i} bbox")
    if dl > LM_TOL_PX: fail(f"face {i} landmarks")
    if ds > SCORE_TOL: fail(f"face {i} score")

if (rust["embedding"] is None) != (cv["embedding"] is None):
    fail("embedding presence differs")
elif rust["embedding"] is not None:
    cos = float(np.dot(rust["embedding"], cv["embedding"]))
    print(f"embedding cosine(rust, opencv) = {cos:.5f}")
    if cos < EMB_COS_MIN: fail("embedding")

print("PASS" if ok else "FAILED")
sys.exit(0 if ok else 1)
