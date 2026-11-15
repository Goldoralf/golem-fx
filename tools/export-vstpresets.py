#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export golem-fx's factory presets as standard VST3 presets (.vstpreset), the files DAWs list in their
preset browsers.

    python3 tools/export-vstpresets.py                  # every crate -> dist/vstpresets/GoleM/<Plugin>/
    python3 tools/export-vstpresets.py --crate gfx-delay
    python3 tools/export-vstpresets.py --out DIR
    python3 tools/export-vstpresets.py --install        # also copy into this OS's user VST3 preset folder

Inputs: presets/<crate>.json (the source of truth), visuals/data/descriptors.json (plugin name, class id,
ranges, enum labels, defaults) and visuals/data/manifest.json (which numeric parameters are IntParams).

A .vstpreset (Steinberg's format, vstpresetfile.cpp in the VST3 SDK, little-endian):

    'VST3'  int32 version (1)  char[32] class id (ASCII hex)  int64 offset of the chunk list
    'Comp' data: the component state, what IComponent::setState receives
    'Info' data: XML meta information (name, comment)
    'List'  int32 count, then per chunk: char[4] id, int64 offset, int64 size

The component state is nih-plug's own (src/wrapper/state.rs, no zstd): the JSON of `PluginState`
{"version", "params": {id: {"f32"|"i32"|"bool": value}}, "fields": {}}, plain (unnormalised) values; an
EnumParam is stored as its variant index ({"i32": rank}, which nih-plug's deserializer accepts as well as
the variant's string id). nih-plug leaves a parameter missing from the map untouched, so every parameter is
written: the preset's value, the plugin's default otherwise. Loading a preset therefore resets the whole
pedal, as golem does. No 'Cont' chunk: nih-plug's controller state is empty (IEditController::setState
ignores it) and the SDK's loader skips a missing one.

The class id string is the 16 ASCII bytes of VST3_CLASS_ID in hex: what the SDK's FUID::toString prints
for nih-plug's class id on every platform (nih-plug swaps the bytes on Windows precisely so that the
string, and so presets and projects, match across platforms).

Fails, without writing anything, on a parameter id the plugin does not have, an enum label that is not an
option or a value out of range. A preset that leaves `bypass` out loads active (its default is off); one
that sets it on on purpose (gfx-tuner's "Drift watch": put away, still measuring) keeps it.
"""

import argparse
import json
import os
import platform
import re
import shutil
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from xml.sax.saxutils import quoteattr

ROOT = Path(__file__).resolve().parent.parent
PRESETS = ROOT / "presets"
DATA = ROOT / "visuals" / "data"
VENDOR = "GoleM"  # every plugin's Plugin::VENDOR: the folder hosts look under
UNSAFE = re.compile(r'[/\\:*?"<>|\x00-\x1f]')


def user_preset_dir() -> Path:
    """The per-user VST3 preset root of this OS (VST3 SDK, "Preset Locations")."""
    home = Path.home()
    system = platform.system()
    if system == "Windows":
        return Path(os.environ.get("USERPROFILE", home)) / "Documents" / "VST3 Presets"
    if system == "Darwin":
        return home / "Library" / "Audio" / "Presets"
    return home / ".vst3" / "presets"


def crate_version(crate: str) -> str:
    text = (ROOT / crate / "Cargo.toml").read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    return m.group(1) if m else ""


def class_id_string(class_id: str) -> str:
    raw = class_id.encode("ascii")
    if len(raw) != 16:
        raise ValueError(f"class id {class_id!r} is not 16 ASCII bytes")
    return raw.hex().upper()


def safe_filename(name: str, taken: set) -> str:
    base = UNSAFE.sub("-", name).strip().rstrip(".") or "Preset"
    cand, n = base, 2
    while cand.casefold() in taken:  # case-insensitive file systems (Windows, macOS)
        cand = f"{base} ({n})"
        n += 1
    taken.add(cand.casefold())
    return cand


def resolve(crate: str, preset: dict, desc: dict, kinds: dict, errors: list) -> dict:
    """The full nih-plug params map of one preset: {param id: {"f32"|"i32"|"bool": value}}."""
    where = f"{crate}/{preset.get('id', '?')}"
    given = preset.get("params", {})
    for pid in given:
        if pid not in desc["params"]:
            errors.append(f"{where}: unknown parameter {pid!r}")
    out = {}
    for pid, q in desc["params"].items():
        kind = kinds.get(pid, q["kind"])
        lo, hi = q["min"], q["max"]
        has = pid in given
        v = given.get(pid)
        if q["kind"] == "enum":
            opts = q.get("options") or [str(x) for x in range(int(lo), int(hi) + 1)]
            if has:
                if not isinstance(v, str) or v not in opts:
                    errors.append(f"{where}: {pid}={v!r} is not an option {opts}")
                    continue
                rank = opts.index(v)
            else:
                rank = int(q["default"])
            # an IntParam shown as a choice stores its plain value, an EnumParam its variant index
            out[pid] = {"i32": int(lo) + rank} if kind == "int" else {"i32": rank}
        elif q["kind"] == "bool":
            if has and not isinstance(v, bool):
                errors.append(f"{where}: {pid} must be true / false, not {v!r}")
                continue
            out[pid] = {"bool": bool(v) if has else bool(q["default"])}
        else:
            if has:
                if not isinstance(v, (int, float)) or isinstance(v, bool) or not (lo <= v <= hi):
                    errors.append(f"{where}: {pid}={v!r} outside {lo}..{hi}")
                    continue
            else:
                v = q["default"]
            if kind == "int":
                if float(v) != int(v):
                    errors.append(f"{where}: {pid}={v!r} must be a whole number")
                    continue
                out[pid] = {"i32": int(v)}
            else:
                out[pid] = {"f32": float(v)}
    return out


def info_xml(plugin: str, preset: dict) -> bytes:
    attrs = [
        ("MediaType", "VstPreset"),
        ("PlugInName", plugin),
        ("Name", preset["name"]),
        ("MediaComment", preset.get("desc", "")),
    ]
    lines = ['<?xml version="1.0" encoding="utf-8"?>', "<MetaInfo>"]
    for k, v in attrs:
        if v:
            lines.append(f'\t<Attr id="{k}" value={quoteattr(v)} type="string" flags="writeProtected"/>')
    lines.append("</MetaInfo>")
    data = ("\n".join(lines) + "\n").encode("utf-8")
    ET.fromstring(data)  # well-formed, or fail here
    return data


def vstpreset(class_id: str, comp: bytes, info: bytes) -> bytes:
    header_size = 4 + 4 + 32 + 8
    chunks = [(b"Comp", comp), (b"Info", info)]
    body, entries, offset = b"", [], header_size
    for cid, data in chunks:
        entries.append((cid, offset, len(data)))
        body += data
        offset += len(data)
    out = b"VST3" + struct.pack("<i", 1) + class_id_string(class_id).encode("ascii") + struct.pack("<q", offset)
    out += body + b"List" + struct.pack("<i", len(entries))
    for cid, off, size in entries:
        out += cid + struct.pack("<qq", off, size)
    return out


def build(crates: list, descs: dict, manifest: dict) -> tuple:
    """{plugin name: {file name: bytes}} and the list of errors."""
    files, errors = {}, []
    for crate in crates:
        desc = descs[crate]
        src = PRESETS / f"{crate}.json"
        if not src.exists():
            continue
        presets = json.loads(src.read_text()).get("presets", [])
        kinds = {p["id"]: p["kind"] for p in manifest.get(crate, [])}
        version = crate_version(crate)
        plugin = desc["name"]
        taken, out = set(), {}
        for p in presets:
            params = resolve(crate, p, desc, kinds, errors)
            state = {"version": version, "params": params, "fields": {}}
            comp = json.dumps(state, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            out[safe_filename(p["name"], taken) + ".vstpreset"] = vstpreset(desc["class_id"], comp, info_xml(plugin, p))
        files[plugin] = out
    return files, errors


def main() -> int:
    ap = argparse.ArgumentParser(description="Export golem-fx's factory presets as .vstpreset files")
    ap.add_argument("--out", default=str(ROOT / "dist" / "vstpresets"), help="output root (default dist/vstpresets)")
    ap.add_argument("--crate", action="append", help="only this crate (gfx-<name>); repeatable")
    ap.add_argument("--install", action="store_true", help="also copy into this OS's user VST3 preset folder")
    args = ap.parse_args()

    descs = json.loads((DATA / "descriptors.json").read_text())["descriptors"]
    manifest = json.loads((DATA / "manifest.json").read_text())["effects"]
    crates = args.crate or list(descs)
    unknown = [c for c in crates if c not in descs]
    if unknown:
        print(f"unknown crate(s): {', '.join(unknown)}", file=sys.stderr)
        return 2

    files, errors = build(crates, descs, manifest)
    if errors:
        print(f"{len(errors)} preset error(s), nothing written:", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1

    out_root = Path(args.out) / VENDOR
    count = 0
    for plugin, entries in files.items():
        d = out_root / plugin
        if d.exists():
            shutil.rmtree(d)  # our own output: no stale renamed presets
        d.mkdir(parents=True)
        for name, data in entries.items():
            (d / name).write_bytes(data)
            count += 1
    print(f"{count} presets of {len(files)} plugins -> {out_root}")

    if args.install:
        dest = user_preset_dir() / VENDOR
        for plugin, entries in files.items():
            (dest / plugin).mkdir(parents=True, exist_ok=True)
            for name in entries:
                shutil.copy2(out_root / plugin / name, dest / plugin / name)
        print(f"installed into {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
