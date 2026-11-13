#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build visuals/data/descriptors.json: every golem-fx pedal's complete, machine-readable descriptor.

golem builds its block sheet from it: the front panel (what the pedal face draws), then the expert
groups with their Custom switches. Sources, from the most to the least authoritative:

  code      data/manifest.json (`gfx-bench manifest`: id, name, kind, range, default, EXP) and each
            crate's src/plugin.rs (plugin name, VST3 class id, units, decimals, enum labels);
  hand      data/faces.json (the front panel) and data/groups.json (the expert groups);
  docs      the parameter tables of each crate's README.md / PARAMS.md / VOICING.md (display taper,
            one-line help, `depends:` notes, reference presets).

Every rule golem relies on is checked; the build fails on any violation:
  - every parameter is in the front panel or in exactly one group, and nothing else is;
  - ids and names are the plugin's; enum labels match the option count;
  - a group's `custom` is a bool parameter of that group; every `depends` names a real parameter.

    python3 visuals/tools/build-descriptors.py            # writes data/descriptors.json
    python3 visuals/tools/build-descriptors.py --check    # verifies the committed file is current
    python3 visuals/tools/build-descriptors.py --manifest # regenerates the manifest first (cargo)
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "visuals" / "data"


def _load_build_data():
    import importlib.util
    spec = importlib.util.spec_from_file_location("build_data", Path(__file__).resolve().parent / "build-data.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------- code: plugin.rs ----------

def plugin_meta(crate: str) -> dict:
    text = (ROOT / crate / "src" / "plugin.rs").read_text()
    name = re.search(r'const NAME: &\'static str = "([^"]+)"', text).group(1)
    cid = re.search(r'const VST3_CLASS_ID: \[u8; 16\] = \*b"([^"]+)"', text).group(1)
    # struct field -> id
    fields = {m.group(2): m.group(1) for m in re.finditer(r'#\[id\s*=\s*"(\w+)"\]\s*(?:pub\s+)?(\w+)\s*:', text)}
    body = re.search(r"impl Default for \w+Params \{.*?Self \{(.*?)\n        \}\n    \}", text, re.S)
    units = {}
    if body:
        b = body.group(1)
        starts = [(m.start(), m.group(1)) for m in re.finditer(r"\n {12}(\w+):", b)]
        for k, (pos, field) in enumerate(starts):
            expr = b[pos:starts[k + 1][0] if k + 1 < len(starts) else len(b)]
            pid = fields.get(field, field)
            units[pid] = unit_of(expr)
    ints = {i: True for f, i in fields.items() if re.search(r"\b" + re.escape(f) + r"\s*:\s*IntParam\b", text)}
    return {"name": name, "class_id": cid, "units": units, "ints": ints}


GATE_HELPERS = {"db(": (" dB", 1), "ms(": (" ms", 1), "hz(": (" Hz", 0)}


def unit_of(expr: str):
    """(unit, decimals) of one parameter's constructor expression."""
    s = expr.strip()
    m = re.search(r'\bf\([^,]+,\s*"([^"]*)",\s*(\d+)\)', s)
    if m:
        return m.group(1), int(m.group(2))
    m = re.search(r'\blin\("[^"]*",[^,]+,[^,]+,[^,]+,\s*"([^"]*)",\s*(\d+)\)', s)
    if m:
        return m.group(1), int(m.group(2))
    for h, (u, d) in GATE_HELPERS.items():
        if re.match(r"\w+:\s*" + re.escape(h), s):
            return u, d
    if "IntParam" in s or re.match(r"\w+:\s*int\(", s):
        return "", 0
    u = re.search(r'with_unit\("([^"]*)"\)', s)
    d = re.search(r"v2s_f32_rounded\((\d+)\)", s) or re.search(r"\{v:\.(\d)\}", s)
    if u or d:
        return (u.group(1) if u else ""), (int(d.group(1)) if d else None)
    return "", None


# ---------- docs: README tables ----------

def doc_files(crate: str):
    files = [ROOT / crate / n for n in ("PARAMS.md", "README.md")]
    if crate in ("gfx-pitch-classic", "gfx-detune"):
        files.append(ROOT / "gfx-pitch" / "README.md")
    return [f for f in files if f.exists()]


def md_plain(s: str) -> str:
    s = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", s)
    s = s.replace("**", "").replace("`", "").replace("\\|", "|")
    return re.sub(r"\s+", " ", s).strip()


def first_sentence(s: str, limit: int = 160) -> str:
    """The first sentence of a role cell, without its cross-references (`depends: …`, `§2`)."""
    s = md_plain(s)
    s = re.sub(r"\s*\((?:depends|[^()]*§)[^()]*\)", "", s)
    depth, cut = 0, len(s)
    for k, ch in enumerate(s):
        depth += ch == "("
        depth -= ch == ")"
        if depth == 0 and ch == "." and (k + 1 == len(s) or s[k + 1] == " ") and not re.search(r"\d$", s[:k]):
            cut = k
            break
    out = s[:cut].strip().rstrip(".;:, ")
    if len(out) > limit:
        out = out[:limit].rsplit(" ", 1)[0] + "…"
    return out


def useful(h):
    """A help line worth showing: not a bare cross-reference (§2) nor a lone word."""
    return h if h and not re.fullmatch(r"§[\d.]+|\S{1,4}", h) else None


def doc_rows(crate: str) -> dict:
    """id -> {curve, role, smooth} from the first table row that documents it."""
    rows = {}
    for f in doc_files(crate):
        header = None
        for line in f.read_text().splitlines():
            if not line.startswith("|"):
                header = None
                continue
            cells = [c.strip() for c in re.split(r"(?<!\\)\|", line.strip())[1:-1]]
            if cells and cells[0].lower() in ("id", "`id`"):
                header = [c.lower() for c in cells]
                continue
            if header is None or set(cells[0]) <= set("-: "):
                continue
            m = re.match(r"`(\w+)`$", cells[0])
            if not m or m.group(1) in rows:
                continue
            get = lambda col: cells[header.index(col)] if col in header and header.index(col) < len(cells) else ""
            rows[m.group(1)] = {"curve": get("curve"), "role": get("role"), "smooth": get("smooth")}
    return rows


def depends_of(role: str, labels: dict, kinds: dict):
    m = re.search(r"depends:\s*`?(\w+)\s*(=|≠|!=)\s*([^`)\s,;]+)", md_plain(role) if "`" not in role else role.replace("`", ""))
    if not m or m.group(2) != "=":
        return None
    pid, val = m.group(1), m.group(3)
    if pid not in kinds:
        return None
    if kinds[pid] == "bool":
        return {"param": pid, "equals": val.lower() in ("on", "true", "1")}
    opts = labels.get(pid) or []
    hit = next((o for o in opts if o.lower() == val.lower()), None)
    return {"param": pid, "equals": hit} if hit else None


# ---------- docs: presets ----------

def chorus_presets(labels: dict) -> list:
    text = (ROOT / "gfx-chorus" / "VOICING.md").read_text()
    sec = text.split("## 6.", 1)[1].split("\n## ", 1)[0]
    out = []
    for line in sec.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 7 or cells[0] in ("Preset",) or set(cells[0]) <= set("-: "):
            continue
        name, profile, mode, rate, depth, mix = cells[:6]
        num = lambda s: float(re.match(r"[\d.]+", s).group(0))
        modes, rates, depths = [m.strip() for m in mode.split("/")], rate.split("/"), depth.split("/")
        names = [name] if len(modes) == 1 else [f"Dimension {m.split()[-1]}" for m in modes]
        if len(modes) > 1:
            modes = [modes[0]] + [f"Dimension {m.strip()}" for m in modes[1:]]
        for k, nm in enumerate(names):
            p = {"profile": profile, "mode": modes[k], "rate": num(rates[min(k, len(rates) - 1)].strip()),
                 "depth": num(depths[min(k, len(depths) - 1)].strip()), "mix": num(mix)}
            if "mix_law" in mix:
                p["mix_law"] = re.search(r"mix_law`?\s*(\w+)", mix).group(1)
            for pid in ("profile", "mode", "mix_law"):
                if pid in p and p[pid] not in labels.get(pid, []):
                    raise SystemExit(f"gfx-chorus preset {nm}: {pid}={p[pid]!r} is not an option")
            out.append({"name": nm, "params": p})
    return out


# ---------- build ----------

PRESETS = ROOT / "presets"
INSTS = {"guitar", "bass", "voice", "keys", "drums", "any"}
GAINS = {"clean", "crunch", "high", "any"}
USES = {"rhythm", "lead", "solo", "ambient", "texture", "utility"}
TAGS = {"vintage", "modern", "subtle", "extreme", "lo-fi", "wide", "dark", "bright", "warm", "aggressive", "dreamy", "tight"}


def validate_presets(crate, presets, params, labels, blocks, errors):
    """The rules of presets/README.md; appends to `errors`."""
    genres = json.loads((PRESETS / "genres.json").read_text())["families"]
    seen, defaults = set(), set()
    for k, p in enumerate(presets):
        where = f"{crate} preset {p.get('id', k)}"
        pid = p.get("id", "")
        if not re.fullmatch(r"[a-z0-9-]{2,48}", pid):
            errors.append(f"{where}: bad id")
        if pid in seen:
            errors.append(f"{where}: duplicate id")
        seen.add(pid)
        if p.get("type") not in blocks:
            errors.append(f"{where}: type {p.get('type')!r} is not one of {blocks}")
        if not (0 < len(p.get("name", "")) <= 40):
            errors.append(f"{where}: name missing or over 40")
        if not (0 < len(p.get("desc", "")) <= 120):
            errors.append(f"{where}: desc missing or over 120")
        if p.get("inst") not in INSTS:
            errors.append(f"{where}: inst {p.get('inst')!r}")
        g = p.get("genres", [])
        if not (1 <= len(g) <= 3) or any(x not in genres for x in g):
            errors.append(f"{where}: genres {g} (1–3 family ids)")
        sub = p.get("subgenres", [])
        if len(sub) > 3 or any(not any(x in genres[f] for f in g) for x in sub):
            errors.append(f"{where}: subgenres {sub} (≤ 3, of its families)")
        if p.get("gain") not in GAINS:
            errors.append(f"{where}: gain {p.get('gain')!r}")
        if p.get("use") not in USES:
            errors.append(f"{where}: use {p.get('use')!r}")
        refs = p.get("refs", [])
        if len(refs) > 3:
            errors.append(f"{where}: more than 3 refs")
        for r in refs:
            if not r.get("artist") or not r.get("why") or len(r["why"]) > 80:
                errors.append(f"{where}: a ref needs artist and a why ≤ 80")
            if r.get("verified") and not r.get("source"):
                errors.append(f"{where}: verified ref without source ({r.get('artist')})")
            if "verified" not in r:
                errors.append(f"{where}: ref without 'verified' ({r.get('artist')})")
        if not isinstance(p.get("level_db"), (int, float)):
            errors.append(f"{where}: level_db missing")
        tags = p.get("tags", [])
        if len(tags) > 3 or any(t not in TAGS for t in tags):
            errors.append(f"{where}: tags {tags}")
        if len(p.get("notes", "")) > 200:
            errors.append(f"{where}: notes over 200")
        if p.get("default"):
            key = (p.get("type"), p.get("inst"))
            if key in defaults:
                errors.append(f"{where}: second default for {key}")
            defaults.add(key)
        for k2, v in p.get("params", {}).items():
            q = params.get(k2)
            if q is None:
                errors.append(f"{where}: unknown param {k2}")
            elif q["kind"] == "enum":
                # an IntParam exposed as a choice has its values as labels (as in the descriptor)
                opts = labels.get(k2) or [str(x) for x in range(int(q["min"]), int(q["max"]) + 1)]
                if v not in opts:
                    errors.append(f"{where}: {k2}={v!r} is not an option {opts}")
            elif q["kind"] == "bool":
                if not isinstance(v, bool):
                    errors.append(f"{where}: {k2} must be true / false")
            else:
                if not isinstance(v, (int, float)) or isinstance(v, bool) or not (q["min"] <= v <= q["max"]):
                    errors.append(f"{where}: {k2}={v!r} outside {q['min']}..{q['max']}")
        if not p.get("params"):
            errors.append(f"{where}: no params")


def complete(params, manifest_params, labels):
    """Every parameter of the pedal: the preset's own values, the plugin's defaults for the rest. An
    exported preset is the whole sound, whatever was loaded before it."""
    out = {}
    for pid, q in manifest_params.items():
        if pid in params:
            out[pid] = params[pid]
        elif q["kind"] == "enum":
            opts = labels.get(pid) or [str(x) for x in range(int(q["min"]), int(q["max"]) + 1)]
            out[pid] = opts[int(q["default"])]
        elif q["kind"] == "bool":
            out[pid] = bool(q["default"])
        else:
            out[pid] = q["default"]
    return out


def file_presets(crate, kinds, labels, blocks, errors, manifest_params):
    f = PRESETS / f"{crate}.json"
    if not f.exists():
        return None
    presets = json.loads(f.read_text()).get("presets", [])
    validate_presets(crate, presets, manifest_params, labels, blocks, errors)
    return [dict(p, params=complete(p.get("params", {}), manifest_params, labels)) for p in presets]


def hand_presets(crate, kinds, labels, errors):
    out = json.loads((DATA / "groups.json").read_text()).get("presets", {}).get(crate, [])
    for p in out:
        for pid, v in p["params"].items():
            if pid not in kinds:
                errors.append(f"{crate}: preset {p['name']} sets unknown {pid}")
            elif kinds[pid] == "enum" and v not in (labels.get(pid) or []):
                errors.append(f"{crate}: preset {p['name']}: {pid}={v!r} is not an option")
    return out


def display_of(curve: str, unit: str, lo, hi) -> str:
    c = curve.strip().lower()
    if c in ("lin", "log"):
        return c
    u = unit.strip()
    if u in ("Hz", "kHz", "ms", "s") and lo and lo > 0 and hi / lo >= 20:
        return "log"
    return "lin"


def build(manifest: dict) -> tuple:
    bd = _load_build_data()
    faces = json.loads((DATA / "faces.json").read_text())["faces"]
    groups_all = json.loads((DATA / "groups.json").read_text())["groups"]
    errors, out = [], {}
    for crate, params in manifest["effects"].items():
        meta = plugin_meta(crate)
        labels = bd.enum_labels(crate)
        docs = doc_rows(crate)
        face = faces.get(crate)
        if face is None:
            errors.append(f"{crate}: no face in faces.json")
            continue
        face_ids = set(face["knobs"]) | set(face.get("selectors", [])) | set(face.get("toggles", []))
        ids = [p["id"] for p in params]
        kinds = {p["id"]: p["kind"] for p in params}
        front = [i for i in ids if i in face_ids]
        for i in face_ids - set(ids):
            errors.append(f"{crate}: face draws {i}, which the plugin does not have")
        groups = groups_all.get(crate)
        if groups is None:
            errors.append(f"{crate}: no entry in groups.json")
            groups = []
        placed = {i: "front" for i in front}
        for g in groups:
            for i in g["params"]:
                if i not in kinds:
                    errors.append(f"{crate}: group {g['id']} lists {i}, which the plugin does not have")
                elif i in placed:
                    errors.append(f"{crate}: {i} is in {placed[i]} and in group {g['id']}")
                else:
                    placed[i] = g["id"]
            c = g.get("custom")
            if c is not None and (c not in g["params"] or kinds.get(c) != "bool"):
                errors.append(f"{crate}: group {g['id']}'s custom {c} must be a bool parameter of the group")
        for i in ids:
            if i not in placed:
                errors.append(f"{crate}: {i} is neither on the front panel nor in a group")

        help_extra = json.loads((DATA / "groups.json").read_text()).get("help", {}).get(crate, {})
        dep_extra = json.loads((DATA / "groups.json").read_text()).get("depends", {}).get(crate, {})
        custom_of = {i: g["custom"] for g in groups if g.get("custom") for i in g["params"] if i != g["custom"]}
        pout = {}
        for p in params:
            i, kind = p["id"], p["kind"]
            unit, dec = meta["units"].get(i, ("", None))
            unit = unit.strip() or None
            d = docs.get(i, {})
            entry = {
                "name": p["name"],
                "kind": "float" if kind == "int" else kind,
                "min": p["min"], "max": p["max"], "default": p["default"],
                "unit": unit if kind in ("float", "int") else None,
                "display": display_of(d.get("curve", ""), unit or "", p["min"], p["max"]) if kind in ("float", "int") else None,
                "step": 1 if kind in ("int", "enum") or meta["ints"].get(i) else None,
                "decimals": 0 if kind in ("int", "enum", "bool") else dec,
                "options": None,
                "exp": bool(p.get("exp")),
                "structural": kind == "enum",
                "depends": None,
                "help": help_extra.get(i) or useful(first_sentence(d["role"]) if d.get("role") else None),
            }
            if kind == "float" and dec is None:
                errors.append(f"{crate}: {i} has no unit / decimals: plugin.rs's constructor was not read (one field per line)")
            if kind == "enum":
                opts = labels.get(i)
                if opts is None and meta["ints"].get(i):
                    # an IntParam the bench exposes as a choice: its values, as the plugin prints them
                    opts = [str(v) for v in range(int(p["min"]), int(p["max"]) + 1)]
                if not opts or len(opts) != p.get("options"):
                    errors.append(f"{crate}: {i} has {p.get('options')} options, plugin.rs labels {opts}")
                entry["options"] = opts
            dep = depends_of(d.get("role", ""), labels, kinds)
            hand = dep_extra.get(i)
            if hand:
                opts = labels.get(hand["param"]) or []
                if kinds.get(hand["param"]) == "enum" and hand["equals"] not in opts:
                    errors.append(f"{crate}: {i} depends on {hand['param']} = {hand['equals']!r}, not one of {opts}")
                dep = hand
            if i in custom_of:
                entry["depends"] = {"param": custom_of[i], "equals": True}
            elif dep:
                entry["depends"] = dep
            if entry["depends"] and entry["depends"]["param"] not in kinds:
                errors.append(f"{crate}: {i} depends on unknown {entry['depends']['param']}")
            pout[i] = entry
        out[crate] = {
            "name": meta["name"], "class_id": meta["class_id"], "blocks": face["blocks"],
            "front": front,
            "groups": [{"id": g["id"], "name": g["name"], "custom": g.get("custom"), "params": g["params"]} for g in groups],
            "params": pout,
            "presets": (file_presets(crate, kinds, labels, face["blocks"], errors, {p["id"]: p for p in params})
                        or (chorus_presets(labels) if crate == "gfx-chorus" else hand_presets(crate, kinds, labels, errors))),
        }
    return out, errors


def render(desc: dict) -> str:
    head = {"_comment": "Generated by visuals/tools/build-descriptors.py from the code (gfx-bench manifest, plugin.rs), "
                        "faces.json, groups.json and each crate's parameter tables. Do not edit.",
            "descriptors": desc}
    return json.dumps(head, ensure_ascii=False, indent=1) + "\n"


def validate_file(path):
    """Checks one presets/<crate>.json against the crate's code; prints the errors."""
    bd = _load_build_data()
    crate = Path(path).stem
    manifest = json.loads((DATA / "manifest.json").read_text())["effects"]
    faces = json.loads((DATA / "faces.json").read_text())["faces"]
    if crate not in manifest or crate not in faces:
        print(f"{crate}: unknown crate")
        return 1
    params = {p["id"]: p for p in manifest[crate]}
    errors = []
    presets = json.loads(Path(path).read_text()).get("presets", [])
    validate_presets(crate, presets, params, bd.enum_labels(crate), faces[crate]["blocks"], errors)
    print("\n".join(errors) if errors else f"{crate}: {len(presets)} presets OK")
    return 1 if errors else 0


def main(argv):
    if "--validate" in argv:
        return max(validate_file(f) for f in argv[argv.index("--validate") + 1:])
    if "--manifest" in argv:
        res = subprocess.run(["cargo", "run", "-q", "-p", "gfx-bench", "--", "manifest"], cwd=ROOT, capture_output=True, text=True)
        if res.returncode:
            print(res.stderr, file=sys.stderr)
            return 1
        (DATA / "manifest.json").write_text(res.stdout)
    manifest = json.loads((DATA / "manifest.json").read_text())
    desc, errors = build(manifest)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    text = render(desc)
    target = DATA / "descriptors.json"
    if "--check" in argv:
        if not target.exists() or target.read_text() != text:
            print("descriptors.json is out of date: run visuals/tools/build-descriptors.py", file=sys.stderr)
            return 1
        print("descriptors.json is current")
        return 0
    target.write_text(text)
    n = sum(len(d["params"]) for d in desc.values())
    nohelp = [f"{c}.{i}" for c, d in desc.items() for i, p in d["params"].items() if not p["help"]]
    print(f"{len(desc)} pedals, {n} parameters, {sum(len(d['groups']) for d in desc.values())} groups; "
          f"{sum(len(d['presets']) for d in desc.values())} presets; {len(nohelp)} without help")
    if nohelp:
        print("  no help: " + ", ".join(nohelp))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
