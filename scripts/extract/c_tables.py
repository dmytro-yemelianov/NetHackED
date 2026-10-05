#!/usr/bin/env python3
"""Extract NetHack 5.0 static tables (objects.h, monsters.h) into TOML data.

The C headers are only *text-expanded* with the system C preprocessor
(`cc -E`): we define the table macros (OBJECT, MON, ...) ourselves so every
record comes out as a brace tree with symbolic names intact. Nothing is
compiled or executed. Every symbol is validated against the identifiers the C
headers define; an unknown symbol aborts with its source line.

Usage: c_tables.py [--check]   (writes data/nethack-5.0/{objects,monsters}.toml)
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
C = ROOT / "NetHack-5.0.0"
INC = C / "include"
OUT = ROOT / "data" / "nethack-5.0"
C_TAG = "NetHack-5.0.0_Released"  # github.com/NetHack/NetHack tag the tree must match
DEFINES = ["MAIL_STRUCTURES"]  # unconditional in include/global.h
EXPECT_OBJECTS = 463  # objects[] minus strange object, generics and terminator
EXPECT_MONSTERS = 383


def die(msg):
    sys.exit(f"c_tables: {msg}")


# ---------------------------------------------------------------- C facts

def header_defs():
    """All identifiers the relevant headers define, with integer values where known."""
    names, values = set(), {}
    for h in ["objclass.h", "skills.h", "prop.h", "color.h", "monattk.h",
              "monflag.h", "weight.h", "align.h", "defsym.h", "sym.h"]:
        text = (INC / h).read_text(errors="replace")
        for m in re.finditer(r"^\s*#\s*define\s+([A-Z_][A-Z0-9_]*)\s+(\S+)", text, re.M):
            names.add(m[1])
            v = m[2].strip("()").rstrip("L").rstrip("U")
            if re.fullmatch(r"-?(0x[0-9a-fA-F]+|\d+)", v):
                values[m[1]] = int(v, 0)
            elif v in values:
                values[m[1]] = values[v]
        for m in re.finditer(r"^\s*([A-Z_][A-Z0-9_]*)\s*(?:=\s*(-?\d+)\s*,?|,)\s*(?:/\*.*)?$", text, re.M):
            names.add(m[1])
            if m[2] is not None:
                values[m[1]] = int(m[2])
    for m in re.finditer(r"OBJCLASS2?\(\s*(\d+),\s*'(.)',\s*(\w+)", (INC / "defsym.h").read_text()):
        names.add(m[3] + "_CLASS")
        values[m[3] + "_CLASS"] = int(m[1])
        OBJ_GLYPH[m[3].lower()] = m[2]
    return names, values


OBJ_GLYPH = {}
NAMES, VALUES = header_defs()
COLOR_ALIAS = dict(re.findall(r"#define\s+(\w+)\s+(CLR_\w+)", (INC / "color.h").read_text()))
MONSYM = {m[2]: (m[1], m[0]) for m in re.findall(
    r"MONSYM\(\s*\d+,\s*'(\\?.)',\s*(\w+),\s*(S_\w+)", (INC / "defsym.h").read_text())}
# MONSYM tuple: S_X -> (BASENAME, glyph)
MONSYM = {k: (v[0], v[1].lstrip('\\')) for k, v in MONSYM.items()}
NAMES |= set(MONSYM)


# ------------------------------------------------------------ preprocessing

def cpp(source: str) -> str:
    args = ["cc", "-E", "-P", "-x", "c", f"-I{INC}"] + [f"-D{d}" for d in DEFINES] + ["-"]
    r = subprocess.run(args, input=source, capture_output=True, text=True)
    if r.returncode != 0:
        die("cc -E failed:\n" + r.stderr[-2000:])
    return r.stdout


TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|0x[0-9a-fA-F]+[UL]*|\d+[UL]*|[A-Za-z_]\w*|@@|[{}(),|*+\-]|\S')


def parse_records(text: str, tag: str):
    """Yield brace trees following each `@@ tag` marker."""
    toks = TOKEN.findall(text)
    i = 0
    while i < len(toks):
        if toks[i] == "@@" and toks[i + 1] == tag:
            node, i = brace(toks, i + 2)
            yield node
        else:
            i += 1


def brace(toks, i):
    """Parse `{ a, {..}, b }` into nested lists; scalars are token lists."""
    assert toks[i] == "{", toks[i:i + 5]
    i += 1
    items, cur = [], []
    while True:
        t = toks[i]
        if t == "{":
            node, i = brace(toks, i)
            cur = node
            continue
        if t in (",", "}"):
            items.append(cur)
            cur = []
            i += 1
            if t == "}":
                return items, i
            continue
        cur.append(t)
        i += 1


def scalar(toks, where):
    """Token list -> str | None | int | list of (sign, ident-or-int) terms."""
    toks = [t for t in toks if t not in ("(", ")")]
    if toks == ["char", "*", "0"] or toks == []:
        return None
    if not toks[0].startswith('"') and any(t in "/*+<>=" for t in toks):
        # arithmetic or comparison (e.g. HARDGEM(n) -> (n >= 8), 200 / 20 + 5):
        # evaluate as C integer arithmetic over literals and known constants
        expr = []
        for t in toks:
            if re.fullmatch(r"\d+[UL]*", t):
                expr.append(t.rstrip("UL"))
            elif t in VALUES:
                expr.append(str(VALUES[t]))
            elif t in "+-*<>=":
                expr.append(t)
            elif t == "/":
                expr.append("//")
            else:
                die(f"{where}: cannot evaluate {toks}")
        return [(1, int(eval("".join(expr).replace(">//", ">/"), {"__builtins__": {}})))]
    if toks[0].startswith('"'):
        return "".join(bytes(t[1:-1], "utf-8").decode("unicode_escape") for t in toks)
    terms, sign = [], 1
    for t in toks:
        if t == "|":
            continue
        if t == "-":
            sign = -1
            continue
        if re.fullmatch(r"0x[0-9a-fA-F]+[UL]*|\d+[UL]*", t):
            terms.append((sign, int(t.rstrip("UL"), 0)))
        elif re.fullmatch(r"[A-Za-z_]\w*", t):
            if t not in NAMES:
                die(f"{where}: unknown C symbol {t}")
            terms.append((sign, t))
        else:
            die(f"{where}: unexpected token {t!r} in {toks}")
        sign = 1
    return terms


def num(v, where):
    """Resolve a numeric field (ints and #define'd constants, summed)."""
    if v is None:
        return 0
    total = 0
    for sign, t in v:
        if isinstance(t, str):
            if t not in VALUES:
                die(f"{where}: no numeric value for {t}")
            t = VALUES[t]
        total += sign * t
    return total


def sym(v, where, prefix="", suffix=""):
    """A single symbolic value -> lowercase name without C prefix/suffix (or None)."""
    if v is None or v == [(1, 0)]:
        return None
    if len(v) != 1 or not isinstance(v[0][1], str):
        die(f"{where}: expected one symbol, got {v}")
    s = v[0][1]
    if not (s.startswith(prefix) and s.endswith(suffix)):
        die(f"{where}: {s} lacks {prefix}*{suffix}")
    return s[len(prefix):len(s) - len(suffix) if suffix else None].lower()


def flags(v, where, prefix):
    """`A | B | 3` -> (sorted names, integer remainder)."""
    names, rest = [], 0
    for sign, t in v or []:
        if isinstance(t, int):
            rest += sign * t
        elif t.startswith(prefix):
            names.append(t[len(prefix):].lower())
        else:
            die(f"{where}: {t} is not a {prefix}* flag")
    return names, rest


def color(v, where):
    s = v[0][1] if v and isinstance(v[0][1], str) else None
    if s is None:
        return None if num(v, where) == 0 else die(f"{where}: numeric color {v}")
    s = COLOR_ALIAS.get(s, s)
    if s == "NO_COLOR":
        return None
    return sym([(1, s)], where, "CLR_")


def source_lines(path: Path):
    """Map each C enum tag (last macro argument) to the line that ends the record."""
    out = {}
    for n, line in enumerate(path.read_text().splitlines(), 1):
        for m in re.finditer(r"\b([A-Z][A-Z0-9_]*)\s*\)\s*,?\s*(?:/\*.*)?$", line):
            out.setdefault(m[1], n)
    return out


# ------------------------------------------------------------------ objects

OBJ_MACROS = """
#define OBJ(name, desc) { name, desc }
#define BITS(nmkn,mrg,uskn,ctnr,mgc,chrg,uniq,nwsh,big,tuf,dir,sub,mtrl) \\
    { nmkn,mrg,uskn,ctnr,mgc,chrg,uniq,nwsh,big,tuf,dir,sub,mtrl }
#define OBJECT(obj,bits,prp,sym,prob,dly,wt,cost,sdam,ldam,oc1,oc2,nut,color,sn) \\
    @@ OBJECT { obj, bits, prp, sym, prob, dly, wt, cost, sdam, ldam, oc1, oc2, nut, color, sn }
#define MARKER(tag, sn)
"""
BIT_NAMES = ["no_name_known", "mergeable", "uses_known", "container", "magic",
             "charged", "unique", "no_wish", "bimanual", "tough"]


def objects():
    path = INC / "objects.h"
    text = path.read_text()
    start = text.index("#if defined(OBJECTS_DESCR_INIT)")
    end = text.index("\n", text.index("#endif  /* OBJECTS_DESCR_INIT"))
    src = OBJ_MACROS + text[:start] + text[end:]
    lines = source_lines(path)
    out = []
    for r in parse_records(cpp(src), "OBJECT"):
        (name, desc), bits, prp, cls, prob, dly, wt, cost, sdam, ldam, oc1, oc2, nut, col, sn = r
        if len(sn) != 1:
            die(f"objects.h: bad enum tag {sn}")
        sn = sn[0]
        if sn == "STRANGE_OBJECT" or sn.startswith("GENERIC_"):
            continue  # objects[0..MAXOCLASSES): placeholders, "none are actual objects"
        w = f"objects.h:{lines.get(sn, '?')} {sn}"
        b = [scalar(x, w) for x in bits]
        skill = num(b[11], w) if b[11] and isinstance(b[11][0][1], int) else None
        sub = b[11][0] if b[11] else None
        rec = {
            "id": sn,
            "name": scalar(name, w),
            "appearance": scalar(desc, w),
            "class": sym(scalar(cls, w), w, "", "_CLASS"),
            "glyph": None,
            "prob": num(scalar(prob, w), w),
            "delay": num(scalar(dly, w), w),
            "weight": num(scalar(wt, w), w),
            "cost": num(scalar(cost, w), w),
            "damage_small": num(scalar(sdam, w), w),
            "damage_large": num(scalar(ldam, w), w),
            "oc1": num(scalar(oc1, w), w),
            "oc2": num(scalar(oc2, w), w),
            "nutrition": num(scalar(nut, w), w),
            "color": color(scalar(col, w), w),
            "property": sym(scalar(prp, w), w),
            "material": sym(b[12], w),
            "flags": [n for n, v in zip(BIT_NAMES, b[:10]) if num(v, w)],
            "src": f"include/objects.h:{lines.get(sn, 0)}",
        }
        if sub and isinstance(sub[1], str) and sub[1] != "P_NONE":
            if sub[1].startswith("P_"):
                rec["skill"] = sub[1][2:].lower()
                rec["skill_is_ammo"] = sub[0] < 0 or None
            elif sub[1].startswith("ARM_"):
                rec["armor_slot"] = sub[1][4:].lower()
            else:
                die(f"{w}: unknown sub {sub}")
        elif skill:
            die(f"{w}: numeric sub {skill}")
        d = b[10]
        if d and d != [(1, 0)]:
            rec["dir"] = [t.lower() for _, t in d] if all(isinstance(t, str) for _, t in d) else die(f"{w}: dir {d}")
        rec["glyph"] = OBJ_GLYPH[rec["class"]]
        if rec["name"] is None and rec["appearance"] is None:
            die(f"{w}: object with neither name nor appearance")
        out.append(rec)
    return out


# ----------------------------------------------------------------- monsters

MON_MACROS = """
#define MON(nam, sym, lvl, gen, atk, siz, mr1, mr2, flg1, flg2, flg3, d, col, bn) \\
    @@ MON { nam, sym, lvl, gen, atk, siz, mr1, mr2, flg1, flg2, flg3, d, col, bn }
#define NAM(name) { 0, 0, name }
#define NAMS(namm, namf, namn) { namm, namf, namn }
#define LVL(lvl, mov, ac, mr, aln) { lvl, mov, ac, mr, aln }
#define SIZ(wt, nut, snd, siz) { wt, nut, snd, siz }
#define ATTK(at, ad, n, d) { at, ad, n, d }
#define A(a1, a2, a3, a4, a5, a6) { a1, a2, a3, a4, a5, a6 }
#define NO_ATTK { 0, 0, 0, 0 }
#include "monsters.h"
"""
SIZES = {VALUES[k]: k for k in NAMES if k.startswith("MZ_") and k in VALUES and k not in ("MZ_HUMAN",)}


def monsters():
    lines = source_lines(INC / "monsters.h")
    out = []
    for r in parse_records(cpp(MON_MACROS), "MON"):
        nam, s, lvl, gen, atk, siz, mr1, mr2, f1, f2, f3, diff, col, bn = r
        bn = bn[0]
        w = f"monsters.h:{lines.get(bn, '?')} {bn}"
        male, female, neutral = (None if scalar(x, w) == [(1, 0)] else scalar(x, w) for x in nam)
        cls = scalar(s, w)[0][1]
        if cls not in MONSYM:
            die(f"{w}: unknown monster class {cls}")
        lv, mov, ac, mr, aln = (num(scalar(x, w), w) for x in lvl)
        gflags, freq = flags(scalar(gen, w), w, "G_")
        attacks = []
        for a in atk:
            at, ad, n, d = (scalar(x, w) for x in a)
            if num(at, w) == 0 and num(ad, w) == 0 and num(n, w) == 0 and num(d, w) == 0:
                continue
            attacks.append({"at": sym(at, w, "AT_"), "ad": sym(ad, w, "AD_"),
                            "n": num(n, w), "d": num(d, w)})
        wt, nut, snd, size = (scalar(x, w) for x in siz)
        size_sym = size[0][1] if isinstance(size[0][1], str) else SIZES.get(num(size, w))
        rec = {
            "id": bn,
            "name": neutral,
            "name_male": male,
            "name_female": female,
            "class": MONSYM[cls][0].lower(),
            "glyph": MONSYM[cls][1],
            "level": lv, "speed": mov, "ac": ac, "mr": mr, "alignment": aln,
            "gen": gflags, "frequency": freq,
            "attacks": attacks,
            "weight": num(wt, w), "nutrition": num(nut, w),
            "sound": sym(snd, w, "MS_"),
            "size": sym([(1, size_sym)], w, "MZ_"),
            "resists": flags(scalar(mr1, w), w, "MR_")[0],
            "conveys": flags(scalar(mr2, w), w, "MR_")[0],
            "flags1": flags(scalar(f1, w), w, "M1_")[0],
            "flags2": flags(scalar(f2, w), w, "M2_")[0],
            "flags3": flags(scalar(f3, w), w, "M3_")[0],
            "difficulty": num(scalar(diff, w), w),
            "color": color(scalar(col, w), w),
            "src": f"include/monsters.h:{lines.get(bn, 0)}",
        }
        out.append(rec)
    return out


# --------------------------------------------------------------------- TOML

def toml_val(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, list):
        if v and isinstance(v[0], dict):
            return "[\n" + "".join("  { " + ", ".join(f"{k} = {toml_val(x)}" for k, x in d.items()) + " },\n" for d in v) + "]"
        return "[" + ", ".join(toml_val(x) for x in v) + "]"
    raise TypeError(v)


def to_toml(table, records, header):
    out = [f"# {header}", f"# GENERATED by scripts/extract/c_tables.py from NetHack/NetHack tag {C_TAG}.",
           "# Do not edit by hand: change the extractor or override in a rule pack.", ""]
    for r in records:
        out.append(f"[[{table}]]")
        for k, v in r.items():
            if v is None or v == []:
                continue
            out.append(f"{k} = {toml_val(v)}")
        out.append("")
    return "\n".join(out)


def check_unique(records, what):
    seen = set()
    for r in records:
        if r["id"] in seen:
            die(f"duplicate {what} id {r['id']}")
        seen.add(r["id"])


def main():
    objs, mons = objects(), monsters()
    check_unique(objs, "object")
    check_unique(mons, "monster")
    if len(objs) != EXPECT_OBJECTS:
        die(f"{len(objs)} objects, expected {EXPECT_OBJECTS}")
    if len(mons) != EXPECT_MONSTERS:
        die(f"{len(mons)} monsters, expected {EXPECT_MONSTERS}")
    # C keeps objects[] in ascending class order (o_init.c init_objects panics otherwise).
    order = []
    for o in objs:
        if not order or order[-1] != o["class"]:
            if o["class"] in order:
                die(f"object class {o['class']} not contiguous at {o['id']}")
            order.append(o["class"])
    files = {
        "objects.toml": to_toml("object", objs, f"{len(objs)} object types from include/objects.h"),
        "monsters.toml": to_toml("monster", mons, f"{len(mons)} monster species from include/monsters.h"),
    }
    check = "--check" in sys.argv
    stale = []
    for name, text in files.items():
        p = OUT / name
        if check:
            if not p.exists() or p.read_text() != text:
                stale.append(name)
        else:
            OUT.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
    print(f"objects: {len(objs)}  monsters: {len(mons)}")
    if stale:
        die("stale: " + ", ".join(stale) + " (run scripts/extract/c_tables.py)")


if __name__ == "__main__":
    main()
