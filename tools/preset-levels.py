#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Loudness of each factory preset of a pedal re the bypass, to set a preset's level before a tag.

Renders every preset of presets/<crate>.json with gfx-bench on golem's DIs and prints its loudness re
the dry input (gfx-fuzz's proxy: RMS after a 100 Hz first-order high-pass). A preset is levelled when
the mean over the DIs is within about ±2 dB; correct it with the pedal's own level parameter (and move
`dry` with it where the preset mixes the dry). It also flags a preset that stores a footswitch
gesture on (`gesture: true` in descriptors.json), which must never be stored on.

  cargo build --release -p gfx-bench
  python3 tools/preset-levels.py gfx-organ                 # golem's clean and funk DIs
  python3 tools/preset-levels.py gfx-dist a.wav b.wav      # other DIs

Needs numpy and scipy. Run it under a memory cap (systemd-run --user --scope -p MemoryMax=4G …).
"""
import json
import math
import os
import subprocess
import sys
import tempfile

import numpy as np
from scipy.io import wavfile
from scipy.signal import lfilter

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DI_DIR = os.path.expanduser("~/golem/library/di-refs/guitar")


def read(path):
    sr, a = wavfile.read(path)
    kind = a.dtype.kind
    a = a.astype(np.float64)
    if a.ndim > 1:
        a = a[:, 0]
    if kind == "i":
        a = a / (2 ** 15 if np.abs(a).max() < 40000 else 2 ** 31)
    return a, sr


def loud(x, sr):
    k = math.tan(math.pi * 100 / sr)
    b0, a1 = 1 / (1 + k), (k - 1) / (1 + k)
    y = lfilter([b0, -b0], [1, a1], x)
    return 10 * math.log10((y * y).mean() + 1e-30)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    crate = sys.argv[1]
    dis = sys.argv[2:] or [os.path.join(DI_DIR, f) for f in ("clean.wav", "funk.wav")]
    desc = json.load(open(f"{REPO}/visuals/data/descriptors.json"))["descriptors"][crate]["params"]
    presets = json.load(open(f"{REPO}/presets/{crate}.json"))["presets"]
    bench = f"{REPO}/target/release/gfx-bench"

    def value(pid, v):
        d = desc[pid]
        if d["kind"] == "enum":
            return d["options"].index(v)
        if d["kind"] == "bool":
            return 1.0 if v in (True, 1, "on") else 0.0
        return float(v)

    for p in presets:
        for k, v in p["params"].items():
            if desc.get(k, {}).get("gesture") and value(k, v):
                print(f"GESTURE STORED ON: {p['id']} {k}")
    levels = {p["id"]: [] for p in presets}
    tmp = tempfile.mkdtemp()
    for di in dis:
        x, sr = read(di)
        base = loud(x, sr)
        for p in presets:
            out = os.path.join(tmp, "out.wav")
            sets = sum((["--set", f"{k}={value(k, v)}"] for k, v in p["params"].items()), [])
            subprocess.run([bench, "render", crate, "--in", di, "--out", out, *sets], check=True, capture_output=True)
            y, _ = read(out)
            levels[p["id"]].append(loud(y, sr) - base)
    print(f"{'preset':30s} " + " ".join(f"{os.path.basename(d)[:10]:>10s}" for d in dis) + "      mean")
    for p in presets:
        v = levels[p["id"]]
        m = sum(v) / len(v)
        flag = "" if abs(m) <= 2 else "   <- level it"
        print(f"{p['id']:30s} " + " ".join(f"{x:+10.1f}" for x in v) + f" {m:+9.1f}{flag}")


if __name__ == "__main__":
    main()
