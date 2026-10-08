"""Golden reference: run OpenCV's FaceDetectorYN + FaceRecognizerSF and print JSON
in the same shape as `face_dump`. Uses the exact same .onnx files and the same
letterbox (pad right/bottom to 640x640, no upscaling) as the Rust code.

usage: python tools/golden.py <models dir> <image>
"""
import json
import sys

import cv2
import numpy as np

models, path = sys.argv[1], sys.argv[2]
img = cv2.imread(path)  # BGR
h, w = img.shape[:2]
scale = min(1.0, 640 / w, 640 / h)
if scale < 1.0:
    sys.exit("golden inputs must already fit in 640x640 (resampling differs between libraries)")

canvas = np.zeros((640, 640, 3), np.uint8)
canvas[:h, :w] = img

det = cv2.FaceDetectorYN.create(f"{models}/face_detection_yunet_2023mar.onnx", "", (640, 640), 0.9, 0.3, 5000)
rec = cv2.FaceRecognizerSF.create(f"{models}/face_recognition_sface_2021dec.onnx", "")

_, faces = det.detect(canvas)
faces = [] if faces is None else faces
out = {"faces": [], "embedding": None}
for f in faces:
    out["faces"].append({
        "bbox": [float(v) for v in f[0:4]],
        "landmarks": [[float(f[4 + 2 * i]), float(f[5 + 2 * i])] for i in range(5)],
        "score": float(f[14]),
    })
if len(faces):
    best = max(faces, key=lambda f: f[14])
    aligned = rec.alignCrop(img, best)
    if len(sys.argv) > 3:
        cv2.imwrite(sys.argv[3], aligned)
    e = rec.feature(aligned).flatten()
    out["embedding"] = (e / np.linalg.norm(e)).tolist()
print(json.dumps(out))
