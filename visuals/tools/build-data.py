#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Builds visuals/data/visuals-data.js (what the gallery and golem read) and visuals/glyphs/*.svg.

Inputs:
  data/manifest.json     every effect's PARAMS (`cargo run -p gfx-bench -- manifest`, rebuilt with --manifest)
  gfx-*/src/plugin.rs    enum option labels (the `#[name]` of each variant)
  data/faces.json        which parameters each gfx pedal shows
  data/glyphs.json       the glyph set
  data/golem-blocks.json snapshot of golem's catalogue (refreshed with --golem ~/golem)

  python3 visuals/tools/build-data.py [--manifest] [--golem ~/golem]
"""
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VIS = ROOT / "visuals"
DATA = VIS / "data"

# catalogue type -> visual family, for the blocks golem-fx does not draw as gfx pedals
NAM_FAMILY = {
    "pedal-nam": "nam-pedal", "preamp-nam": "nam-pedal", "comp-nam": "nam-pedal",
    "capture-module": "nam-pedal", "amp-nam": "nam-head", "amp": "nam-stack", "cab": "nam-cab",
}
INSTRUMENT_ENGINES = {"rs5k", "drumkit", "b3", "sfizz", "dexed"}
SERVICE_TYPES = {"backing-track", "looper", "rec", "click", "compo", "sample", "drums", "standard", "arrangement"}


def enum_labels(crate: str) -> dict:
    """param id -> option labels, from the crate's plugin.rs (nih-plug Enum derives)."""
    src_dir = ROOT / crate / "src"
    text = "".join(p.read_text() for p in sorted(src_dir.glob("*.rs")))
    sync = (ROOT / "gfx-dsp/src/transport.rs").read_text()
    enums = {}
    for m in re.finditer(r"enum\s+(\w+)\s*\{(.*?)\n\s*\}", text + sync, re.S):
        body = m.group(2)
        labels, pending = [], None
        for line in body.splitlines():
            line = line.strip()
            nm = re.match(r'#\[name\s*=\s*"(.*)"\]', line)
            if nm:
                pending = nm.group(1)
                continue
            vm = re.match(r"([A-Z]\w*)\s*(=\s*\d+\s*)?,?$", line)
            if vm:
                labels.append(pending or vm.group(1))
                pending = None
        if labels:
            enums.setdefault(m.group(1), labels)
    out = {}
    for m in re.finditer(r'#\[id\s*=\s*"(\w+)"\]\s*(?:pub\s+)?\w+\s*:\s*EnumParam<(\w+)>', text):
        if m.group(2) in enums:
            out[m.group(1)] = enums[m.group(2)]
    return out


def main(argv):
    if "--manifest" in argv:
        cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
        out = subprocess.run([cargo, "run", "-q", "--release", "-p", "gfx-bench", "--", "manifest"],
                             cwd=ROOT, check=True, capture_output=True, text=True).stdout
        (DATA / "manifest.json").write_text(out)
    if "--golem" in argv:
        golem = Path(argv[argv.index("--golem") + 1]).expanduser()
        cat = json.loads((golem / "library/catalog/effects.json").read_text())
        keep = ("type", "name", "short", "category", "engine", "plugin", "color", "glyph", "icon")
        snap = {
            "_comment": "Snapshot of golem's catalogue (library/catalog/effects.json): only what the visuals "
                        "need. Refresh: tools/build-data.py --golem ~/golem",
            "catalog_version": cat.get("catalog_version"),
            "effects": [dict({k: e.get(k) for k in keep},
                             params=[{k: p.get(k) for k in ("id", "name", "min", "max", "default", "unit")}
                                     for p in e.get("params", [])][:6]) for e in cat["effects"]],
            "pedals": cat.get("pedals", []), "amps": cat.get("amps", []), "cabs": cat.get("cabs", []),
        }
        (DATA / "golem-blocks.json").write_text(json.dumps(snap, ensure_ascii=False, indent=1) + "\n")

    manifest = json.loads((DATA / "manifest.json").read_text())["effects"]
    faces = json.loads((DATA / "faces.json").read_text())["faces"]
    glyphs = json.loads((DATA / "glyphs.json").read_text())["glyphs"]
    golem = json.loads((DATA / "golem-blocks.json").read_text())
    by_type = {e["type"]: e for e in golem["effects"]}
    errors = []

    gfx = {}
    block_to_gfx = {}
    for crate, face in faces.items():
        params = {p["id"]: p for p in manifest.get(crate, [])}
        if not params:
            errors.append(f"{crate}: not in manifest.json")
            continue
        labels = enum_labels(crate)
        lead = by_type.get(face["blocks"][0], {})

        def pick(ids, kind):
            out = []
            for pid in ids:
                p = params.get(pid)
                if not p:
                    errors.append(f"{crate}: no parameter {pid}")
                    continue
                q = {k: p[k] for k in ("id", "name", "min", "max", "default", "kind")}
                if kind == "selector":
                    if p["kind"] != "enum":
                        errors.append(f"{crate}: selector {pid} is {p['kind']}")
                    opts = labels.get(pid) or [str(i + 1) for i in range(p.get("options", 2))]
                    if len(opts) != p.get("options", len(opts)):
                        errors.append(f"{crate}: {pid} has {p.get('options')} options, plugin.rs {len(opts)}")
                    q["options"] = opts
                out.append(q)
            return out

        gfx[crate] = {
            "crate": crate, "name": face["name"], "blocks": face["blocks"], "glyph": face["blocks"][0],
            "category": face.get("category") or lead.get("category", "utility"),
            "color": face.get("color") or lead.get("color") or "#8a94a3",
            "shape": face.get("shape", "stomp"), "treadle": face.get("treadle"),
            "knobs": pick(face.get("knobs", []), "knob"),
            "selectors": pick(face.get("selectors", []), "selector"),
            "toggles": pick(face.get("toggles", []), "toggle"),
        }
        for b in face["blocks"]:
            block_to_gfx[b] = crate
    for crate in manifest:
        if crate not in faces:
            errors.append(f"{crate}: no face in faces.json")

    blocks = []
    for e in golem["effects"]:
        t = e["type"]
        if t not in glyphs:
            errors.append(f"block {t}: no glyph")
        if t in block_to_gfx:
            fam = "gfx"
        elif t in NAM_FAMILY:
            fam = NAM_FAMILY[t]
        elif t in SERVICE_TYPES:
            fam = "service"
        elif e.get("engine") in INSTRUMENT_ENGINES or e.get("category") in ("keys", "pads", "drums") or t == "vital":
            fam = "instrument"
        else:
            fam = "generic"
        blocks.append({**{k: e[k] for k in ("type", "name", "short", "category", "engine", "plugin", "color")},
                       "glyph": t, "family": fam, "gfx": block_to_gfx.get(t), "params": e.get("params", [])})

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1

    data = {"glyphs": glyphs, "gfx": gfx, "blocks": blocks, "catalog_version": golem.get("catalog_version"),
            "archetypes": dict({k: golem.get(k, []) for k in ("pedals", "amps", "cabs")},
                               outboard=json.loads((DATA / "outboard.json").read_text())["outboard"])}
    (DATA / "visuals-data.js").write_text(
        "// Generated by visuals/tools/build-data.py. Do not edit.\nwindow.GOLEM_VISUALS = "
        + json.dumps(data, ensure_ascii=False, separators=(",", ":")) + ";\n")

    gdir = VIS / "glyphs"
    gdir.mkdir(exist_ok=True)
    for old in gdir.glob("*.svg"):
        old.unlink()
    for gid, g in glyphs.items():
        (gdir / f"{gid}.svg").write_text(
            '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" '
            'stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">'
            f'<title>{g["label"]}</title>{g["svg"]}</svg>\n')
    print(f"{len(gfx)} gfx pedals, {len(blocks)} golem blocks, {len(glyphs)} glyphs")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
