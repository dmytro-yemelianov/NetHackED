#!/usr/bin/env python3
"""Generate crates/nethacked-data/src/bestiary_generated.rs from NetHack 5.0 C.

Reads `NetHack-5.0.0/include/monsters.h` (git-ignored local reference tree)
and emits `MonsterSpeciesId` plus `BESTIARY` for all 394 C species.

The 55 species NetHackED already modelled by hand are copied verbatim from the
current generated file (or, on first run, from `monsters.rs`). Their enum names,
order and modelling fields (`base_hp`, `ai_behavior`, `abilities`, extra
intrinsics) are kept, so existing behaviour and golden fingerprints do not move.
Every other species is generated from its `MON(...)` entry:
- C fields: name, class symbol, level, speed, AC, alignment, attacks, size,
  resistances, flags;
- `base_hp`: the mean of C's creation roll (`makemon.c` `newmonhp`:
  `d(level, 8)`, or `rnd(4)` at level 0), rounded up;
- `ai_behavior`: `Stationary` for speed 0, else `MeleeHunter`;
- `abilities`: none.

Usage: python3 scripts/gen-bestiary.py [--check]
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
C_SRC = ROOT / "NetHack-5.0.0/include/monsters.h"
OUT = ROOT / "crates/nethacked-data/src/bestiary_generated.rs"
LEGACY_SRC = ROOT / "crates/nethacked-data/src/monsters.rs"

SYM = {
    "ANT": "a", "BLOB": "b", "COCKATRICE": "c", "DOG": "d", "EYE": "e", "FELINE": "f", "GREMLIN": "g",
    "HUMANOID": "h", "IMP": "i", "JELLY": "j", "KOBOLD": "k", "LEPRECHAUN": "l", "MIMIC": "m", "NYMPH": "n",
    "ORC": "o", "PIERCER": "p", "QUADRUPED": "q", "RODENT": "r", "SPIDER": "s", "TRAPPER": "t", "UNICORN": "u",
    "VORTEX": "v", "WORM": "w", "XAN": "x", "LIGHT": "y", "ZRUTY": "z", "ANGEL": "A", "BAT": "B", "CENTAUR": "C",
    "DRAGON": "D", "ELEMENTAL": "E", "FUNGUS": "F", "GNOME": "G", "GIANT": "H", "INVISIBLE": "I",
    "JABBERWOCK": "J", "KOP": "K", "LICH": "L", "MUMMY": "M", "NAGA": "N", "OGRE": "O", "PUDDING": "P",
    "QUANTMECH": "Q", "RUSTMONST": "R", "SNAKE": "S", "TROLL": "T", "UMBER": "U", "VAMPIRE": "V", "WRAITH": "W",
    "XORN": "X", "YETI": "Y", "ZOMBIE": "Z", "HUMAN": "@", "GHOST": " ", "GOLEM": "'", "DEMON": "&", "EEL": ";",
    "LIZARD": ":", "WORM_TAIL": "~", "MIMIC_DEF": "]",
}
AT = {
    "AT_NONE": "Passive", "AT_CLAW": "Claw", "AT_BITE": "Bite", "AT_KICK": "Kick", "AT_BUTT": "Butt",
    "AT_TUCH": "Touch", "AT_STNG": "Sting", "AT_HUGS": "Hug", "AT_SPIT": "Spit", "AT_ENGL": "Engulf",
    "AT_BREA": "Breath", "AT_EXPL": "Explode", "AT_BOOM": "Boom", "AT_GAZE": "Gaze", "AT_TENT": "Tentacle",
    "AT_WEAP": "Weapon", "AT_MAGC": "Magic",
}
AD = {
    "AD_PHYS": "Phys", "AD_MAGM": "MagicMissile", "AD_FIRE": "Fire", "AD_COLD": "Cold", "AD_SLEE": "Sleep",
    "AD_DISN": "Disintegrate", "AD_ELEC": "Elec", "AD_DRST": "DrainStr", "AD_ACID": "Acid", "AD_BLND": "Blind",
    "AD_STUN": "Stun", "AD_SLOW": "Slow", "AD_PLYS": "Paralyze", "AD_DRLI": "DrainLife", "AD_DREN": "DrainEnergy",
    "AD_LEGS": "Legs", "AD_STON": "Stone", "AD_STCK": "Sticky", "AD_SGLD": "StealGold", "AD_SITM": "StealItem",
    "AD_SEDU": "Seduce", "AD_TLPT": "Teleport", "AD_RUST": "Rust", "AD_CONF": "Confuse", "AD_DGST": "Digest",
    "AD_HEAL": "Heal", "AD_WRAP": "Wrap", "AD_WERE": "Lycanthropy", "AD_DRDX": "DrainDex", "AD_DRCO": "DrainCon",
    "AD_DRIN": "DrainInt", "AD_DISE": "Disease", "AD_DCAY": "Decay", "AD_SSEX": "SeduceSex", "AD_HALU": "Hallucinate",
    "AD_DETH": "Death", "AD_PEST": "Pestilence", "AD_FAMN": "Famine", "AD_SLIM": "Slime", "AD_ENCH": "Disenchant",
    "AD_CORR": "Corrode", "AD_POLY": "Polymorph", "AD_CLRC": "Clerical", "AD_SPEL": "Spell", "AD_RBRE": "RandomBreath",
    "AD_SAMU": "StealAmulet", "AD_CURS": "Curse", "AD_CNCL": "Cancel",
}
SZ = {"0": "Tiny", "MZ_TINY": "Tiny", "MZ_SMALL": "Small", "MZ_MEDIUM": "Medium", "MZ_HUMAN": "Medium", "MZ_LARGE": "Large",
      "MZ_HUGE": "Huge", "MZ_GIGANTIC": "Gigantic"}
RES = {"MR_FIRE": "fire_resistance", "MR_COLD": "cold_resistance", "MR_SLEEP": "sleep_resistance",
       "MR_POISON": "poison_resistance", "MR_ELEC": "shock_resistance", "MR_DISINT": "disintegration_resistance",
       "MR_ACID": "acid_resistance"}
RACE = {"M2_HUMAN": "Human", "M2_ELF": "Elf", "M2_DWARF": "Dwarf", "M2_GNOME": "Gnome", "M2_ORC": "Orc"}
SOUND = {"MS_LEADER": "Leader", "MS_GUARDIAN": "Guardian", "MS_NEMESIS": "Nemesis"}


def split_args(s: str) -> list[str]:
    """Split a macro argument list on top-level commas."""
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


# Feature macros as configured by NetHack 5.0 (include/global.h:430 defines
# MAIL_STRUCTURES; CHARON is not defined anywhere).
DEFINED = {"MAIL_STRUCTURES"}


def preprocess(text: str) -> str:
    """Drop lines in inactive #if/#ifdef blocks and expand object-like #defines.

    Line numbers are kept (inactive lines become blank) so provenance stays exact.
    """
    lines = text.split("\n")
    out, stack, macros = [], [], {}
    i = 0
    while i < len(lines):
        line = lines[i]
        s = line.strip()
        active = all(stack)
        if s.startswith("#if"):
            if s.startswith("#ifdef"):
                cond = s.split()[1] in DEFINED
            elif s.startswith("#ifndef"):
                cond = s.split()[1] not in DEFINED
            elif re.match(r"#if\s+0\b", s):
                cond = False
            else:  # '#if defined(X) || ...': the MON-generator scaffolding, never a species
                cond = False
            stack.append(cond)
            out.append("")
        elif s.startswith("#else"):
            stack[-1] = not stack[-1]
            out.append("")
        elif s.startswith("#endif"):
            stack.pop()
            out.append("")
        elif s.startswith("#define") and active:
            body = [line]
            while body[-1].rstrip().endswith("\\"):
                i += 1
                body.append(lines[i])
            m = re.match(r"#define\s+([A-Z_][A-Z0-9_]*)\s+(.*)", " ".join(b.rstrip("\\") for b in body).strip(), re.S)
            if m and "(" not in m.group(1):
                macros[m.group(1)] = m.group(2).strip()
            out.extend([""] * len(body))
        else:
            out.append(line if active else "")
        i += 1
    text = "\n".join(out)
    for name, value in macros.items():
        text = re.sub(rf"\b{name}\b", value, text)
    return text


def mon_entries(text: str):
    """Yield (line, argument list) for every top-level MON(...) entry."""
    for m in re.finditer(r"^\s*MON\(", text, re.M):
        start = text.index("(", m.start()) + 1
        depth, i = 1, start
        while depth:
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            i += 1
        body = re.sub(r"/\*.*?\*/", "", text[start:i - 1], flags=re.S)
        yield text.count("\n", 0, m.start()) + 1, split_args(body)


def inner(arg: str, macro: str) -> list[str]:
    m = re.match(rf"{macro}\((.*)\)$", arg.strip(), re.S)
    assert m, (macro, arg)
    return split_args(m.group(1))


def flags(arg: str) -> set[str]:
    return set(re.findall(r"[A-Z][A-Z0-9_]+", arg))


def camel(name: str) -> str:
    return "".join(w[:1].upper() + w[1:] for w in re.split(r"[^A-Za-z0-9]+", name) if w)


def parse_c() -> list[dict]:
    text = preprocess(C_SRC.read_text())
    out, used = [], set()
    for line, a in mon_entries(text):
        # MON(names, sym, LVL(...), gen, A(...), SIZ(...), mr1, mr2, m1, m2, m3, difficulty, color)
        names = a[0]
        if names.startswith("NAMS"):
            male, female, neutral = [s.strip().strip('"') for s in inner(names, "NAMS")]
        else:
            male = neutral = inner(names, "NAM")[0].strip('"')
        sym = a[1].strip()
        assert sym.startswith("S_"), (line, sym)
        base = sym[2:]
        lvl, spd, ac, _mr, aln = [x.strip() for x in inner(a[2], "LVL")]
        name = male
        if name.lower() in used:
            # A repeated name: NAMS entries use their neutral name (player-monster
            # "priest" -> "cleric"); a were's human form gets "human ".
            name = neutral if names.startswith("NAMS") else f"human {male}"
        assert name.lower() not in used, (line, name)
        used.add(name.lower())
        attacks = []
        for at in inner(a[4], "A"):
            if at.strip() == "NO_ATTK":
                continue
            t, d, n, dd = inner(at, "ATTK")
            attacks.append((AT[t.strip()], AD[d.strip()], int(n), int(dd)))
        siz = inner(a[5], "SIZ")
        m1, m2, gen = flags(a[8]), flags(a[9]), flags(a[3])
        res = sorted(RES[r] for r in flags(a[6]) if r in RES)
        if "M1_SEE_INVIS" in m1:
            res.append("see_invisible")
        maligntyp = -128 if aln == "A_NONE" else int(aln)
        alignment = ("Unaligned" if aln == "A_NONE" else "Lawful" if maligntyp > 0
                     else "Chaotic" if maligntyp < 0 else "Neutral")
        race = next((RACE[f] for f in ("M2_ELF", "M2_DWARF", "M2_GNOME", "M2_ORC", "M2_HUMAN") if f in m2), None)
        level = int(lvl)
        out.append(dict(
            line=line, name=name, glyph=SYM[base], level=level, speed=int(spd), ac=int(ac),
            alignment=alignment, maligntyp=max(-128, min(127, maligntyp)), attacks=attacks,
            size=SZ[siz[3].strip()], sound=SOUND.get(siz[2].strip(), "Other"), res=sorted(set(res)),
            peaceful="M2_PEACEFUL" in m2, hostile="M2_HOSTILE" in m2, human="M2_HUMAN" in m2,
            unique="G_UNIQ" in gen, mindless="M1_MINDLESS" in m1, race=race,
            hp=3 if level == 0 else (9 * level + 1) // 2,
        ))
    return out


def legacy_entries() -> tuple[list[tuple[str, str, str]], str]:
    """(variant, name, verbatim entry) for the hand-modelled species, plus enum attributes."""
    src = OUT.read_text() if OUT.exists() else LEGACY_SRC.read_text()
    table = src.split("pub static BESTIARY: &[MonsterArchetype] = &[\n", 1)[1].split("\n];\n", 1)[0] + "\n"
    entries = []
    for m in re.finditer(r"    MonsterArchetype \{\n.*?\n    \},\n", table, re.S):
        body = m.group(0)
        if "// generated from" in body:
            continue
        entries.append((re.search(r"id: MonsterSpeciesId::(\w+)", body).group(1),
                        re.search(r'name: "([^"]*)"', body).group(1), body))
    return entries, src


def rust_entry(variant: str, d: dict) -> str:
    intr = ("Intrinsics::empty()" if not d["res"] else
            "Intrinsics {\n" + "".join(f"            {r}: true,\n" for r in d["res"])
            + "            ..Intrinsics::empty()\n        }")
    atk = "&[]" if not d["attacks"] else "&[\n" + "".join(
        f"            Attack {{ at: AttackType::{a}, ad: DamageType::{b}, n: {n}, d: {dd} }},\n"
        for a, b, n, dd in d["attacks"]) + "        ]"
    glyph = "'\\''" if d["glyph"] == "'" else f"'{d['glyph']}'"
    race = f"Some(RaceId::{d['race']})" if d["race"] else "None"
    ai = "AiBehavior::Stationary" if d["speed"] == 0 else "AiBehavior::MeleeHunter"
    return f"""    MonsterArchetype {{
        // generated from NetHack-5.0.0/include/monsters.h:{d['line']}
        id: MonsterSpeciesId::{variant},
        name: "{d['name']}",
        glyph: {glyph},
        base_hp: {d['hp']},
        max_hp: {d['hp']},
        ac: {d['ac']},
        level: {d['level']},
        speed: {d['speed']},
        alignment: Alignment::{d['alignment']},
        intrinsics: {intr},
        attacks: {atk},
        size: MonsterSize::{d['size']},
        peaceful_by_default: {str(d['peaceful']).lower()},
        always_hostile: {str(d['hostile']).lower()},
        maligntyp: {d['maligntyp']},
        msound: MonsterSound::{d['sound']},
        m2_race: {race},
        is_human: {str(d['human']).lower()},
        is_unique: {str(d['unique']).lower()},
        mindless: {str(d['mindless']).lower()},
        ai_behavior: {ai},
        abilities: &[],
    }},
"""


def render() -> str:
    c = parse_c()
    print(f"C species after preprocessing: {len(c)}")
    legacy, _ = legacy_entries()
    # Legacy display names that differ from C (quest text uses the article).
    aliases = {"the norn": "norn", "the dark one": "dark one"}
    legacy_names = {aliases.get(n.lower(), n.lower()) for _, n, _ in legacy}
    c_names = {d["name"].lower() for d in c}
    assert legacy_names <= c_names, legacy_names - c_names
    variants = [v for v, _, _ in legacy]
    body = [e for _, _, e in legacy]
    for d in c:
        if d["name"].lower() in legacy_names:
            continue
        v = camel(d["name"])
        if v in variants:
            # A legacy variant already uses this name for a different species
            # (legacy `Orc` is "hill orc", `Lich` is "master lich").
            v = "Plain" + v
        assert v not in variants, (v, d["name"])
        variants.append(v)
        body.append(rust_entry(v, d))
    enum = "".join(f"    {v},\n" for v in variants)
    return f"""// @generated by scripts/gen-bestiary.py from NetHack 5.0 include/monsters.h.
// Do not edit by hand: change the script (or the hand-modelled legacy entries,
// which the script copies verbatim) and re-run it.
//
// {len(legacy)} hand-modelled species keep their enum names, order and modelling
// fields; the other {len(variants) - len(legacy)} come straight from C.

/// Every NetHack 5.0 monster species (C `PM_*`), plus NetHackED's hand-modelled ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MonsterSpeciesId {{
{enum}}}

/// Number of species NetHackED modelled by hand before the full C import; the
/// first `LEGACY_SPECIES` entries of [`BESTIARY`] are those, in their old order.
pub const LEGACY_SPECIES: usize = {len(legacy)};

pub static BESTIARY: &[MonsterArchetype] = &[
{"".join(body)}];
"""


def main() -> None:
    out = render()
    if "--check" in sys.argv:
        if OUT.read_text() != out:
            raise SystemExit("bestiary_generated.rs is out of date; run scripts/gen-bestiary.py")
        print("bestiary up to date")
        return
    OUT.write_text(out)
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
