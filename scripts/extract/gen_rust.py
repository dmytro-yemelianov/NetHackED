#!/usr/bin/env python3
"""Generate the Rust static tables from data/nethack-5.0/*.toml.

The TOML is the extracted C data (scripts/extract/c_tables.py); this step only
maps C vocabulary onto the engine's Rust types. Every mapping is exhaustive:
an unknown value aborts. Needs no C tree, so CI runs `--check` on every push.

Usage: gen_rust.py [--check]
"""
import re
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "data" / "nethack-5.0"
OUT = ROOT / "crates" / "nethacked-data" / "src" / "generated"


def die(msg):
    sys.exit(f"gen_rust: {msg}")


def load(name, table):
    return tomllib.loads((DATA / name).read_text())[table]


def camel(name):
    name = re.sub(r"^the ", "", name, flags=re.I)
    return "".join(w[:1].upper() + w[1:] for w in re.split(r"[^A-Za-z0-9]+", name) if w)


def rs(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def pick(table, key, where):
    if key not in table:
        die(f"{where}: no mapping for {key!r}")
    return table[key]


# ----------------------------------------------------------------- monsters

AT = {"none": "Passive", "claw": "Claw", "bite": "Bite", "kick": "Kick", "butt": "Butt", "tuch": "Touch",
      "stng": "Sting", "hugs": "Hug", "spit": "Spit", "engl": "Engulf", "brea": "Breath", "expl": "Explode",
      "boom": "Boom", "gaze": "Gaze", "tent": "Tentacle", "weap": "Weapon", "magc": "Magic"}
AD = {"phys": "Phys", "magm": "MagicMissile", "fire": "Fire", "cold": "Cold", "slee": "Sleep",
      "disn": "Disintegrate", "elec": "Elec", "drst": "DrainStr", "acid": "Acid", "blnd": "Blind", "stun": "Stun",
      "slow": "Slow", "plys": "Paralyze", "drli": "DrainLife", "dren": "DrainEnergy", "legs": "Legs",
      "ston": "Stone", "stck": "Sticky", "sgld": "StealGold", "sitm": "StealItem", "sedu": "Seduce",
      "tlpt": "Teleport", "rust": "Rust", "conf": "Confuse", "dgst": "Digest", "heal": "Heal", "wrap": "Wrap",
      "were": "Lycanthropy", "drdx": "DrainDex", "drco": "DrainCon", "drin": "DrainInt", "dise": "Disease",
      "dcay": "Decay", "ssex": "SeduceSex", "halu": "Hallucinate", "deth": "Death", "pest": "Pestilence",
      "famn": "Famine", "slim": "Slime", "ench": "Disenchant", "corr": "Corrode", "poly": "Polymorph",
      "clrc": "Clerical", "spel": "Spell", "rbre": "RandomBreath", "samu": "StealAmulet", "curs": "Curse",
      "cncl": "Cancel"}
SIZE = {"tiny": "Tiny", "small": "Small", "medium": "Medium", "human": "Medium", "large": "Large",
        "huge": "Huge", "gigantic": "Gigantic"}
RESIST = {"fire": "fire_resistance", "cold": "cold_resistance", "sleep": "sleep_resistance",
          "poison": "poison_resistance", "elec": "shock_resistance", "disint": "disintegration_resistance",
          "acid": "acid_resistance", "stone": None}
RACE = ["elf", "dwarf", "gnome", "orc", "human"]  # C race flag precedence (M2_*)
SOUND = {"leader": "Leader", "guardian": "Guardian", "nemesis": "Nemesis"}
BREATH = {"fire": "Fire", "cold": "Cold", "elec": "Shock", "slee": "Sleep", "drst": "Poison",
          "disn": "Disintegration"}
GAZE = {"plys": "Paralysis", "ston": "Petrification", "conf": "Confusion"}


def monster_names(mons):
    """Unique display names; C names repeat (were-creatures, player monsters)."""
    # Gendered species (NAMS) display their male name, as C does for a male
    # monster; when that name is already taken (player-monster "priest" after
    # the temple priest) the neutral C name is used instead.
    used, out = set(), {}
    for m in mons:
        name = m.get("name_male") or m["name"]
        if name.lower() in used:
            name = m["name"] if m.get("name_male") else f"human {name}"
        if name.lower() in used:
            die(f"{m['src']}: duplicate monster name {name}")
        used.add(name.lower())
        out[m["id"]] = name
    return out


def monster_entry(m, name):
    w = m["src"]
    res = sorted(r for r in (pick(RESIST, x, w) for x in m.get("resists", [])) if r)
    if "see_invis" in m.get("flags1", []):
        res.append("see_invisible")
    intr = ("Intrinsics::empty()" if not res else
            "Intrinsics { " + "".join(f"{r}: true, " for r in sorted(set(res))) + "..Intrinsics::empty() }")
    attacks, abilities = [], []
    for a in m.get("attacks", []):
        attacks.append(f"Attack {{ at: AttackType::{pick(AT, a['at'], w)}, "
                       f"ad: DamageType::{pick(AD, a['ad'], w)}, n: {a['n']}, d: {a['d']} }}")
        if a["at"] == "brea" and a["ad"] in BREATH:
            abilities.append(f"MonsterAbility::Breath {{ breath: BreathType::{BREATH[a['ad']]} }}")
        if a["at"] == "magc" and a["ad"] in ("spel", "clrc"):
            # Engine approximation of C castmu() until it is ported: the strongest
            # spell the caster's level reaches in C's spell table, every 8 turns.
            spell = ("SummonMonsters" if a["ad"] == "spel" and m["level"] >= 18
                     else "CurseItems" if m["level"] >= (13 if a["ad"] == "spel" else 8)
                     else "CauseWounds")
            abilities.append(f"MonsterAbility::Spellcaster {{ spell: MonsterSpell::{spell}, cooldown_turns: 8 }}")
        if a["at"] == "gaze" and a["ad"] in GAZE:
            abilities.append(f"MonsterAbility::Gaze {{ gaze: GazeType::{GAZE[a['ad']]} }}")
    f1, f2 = m.get("flags1", []), m.get("flags2", [])
    aln = m["alignment"]
    alignment = ("Unaligned" if aln == -128 else "Lawful" if aln > 0 else "Chaotic" if aln < 0 else "Neutral")
    race = next((r for r in RACE if r in f2), None)
    level = m["level"]
    hp = 3 if level == 0 else (9 * level + 1) // 2
    # M3_WAITFORU / M3_CLOSE set STRAT_WAITMASK: the monster does not move (monmove.c:717)
    waits = {"waitforu", "close"} & set(m.get("flags3", []))
    ai = ("Stationary" if m["speed"] == 0 or waits else "Shopkeeper" if m["id"] == "SHOPKEEPER"
          else "CompanionPet" if "domestic" in f2 else "MeleeHunter")
    glyph = m["glyph"].replace("\\", "\\\\").replace("'", "\\'")
    return f"""    MonsterArchetype {{
        // {w}
        id: MonsterSpeciesId::{m['id']},
        name: {rs(name)},
        glyph: '{glyph}',
        base_hp: {hp},
        max_hp: {hp},
        ac: {m['ac']},
        level: {level},
        speed: {m['speed']},
        alignment: Alignment::{alignment},
        intrinsics: {intr},
        attacks: &[{", ".join(attacks)}],
        size: MonsterSize::{pick(SIZE, m['size'], w)},
        peaceful_by_default: {str('peaceful' in f2).lower()},
        always_hostile: {str('hostile' in f2).lower()},
        maligntyp: {max(-128, min(127, aln))},
        msound: MonsterSound::{SOUND.get(m.get('sound'), 'Other')},
        m2_race: {f'Some(RaceId::{race.capitalize()})' if race else 'None'},
        is_human: {str('human' in f2).lower()},
        is_unique: {str('uniq' in m.get('gen', [])).lower()},
        mindless: {str('mindless' in f1).lower()},
        ai_behavior: AiBehavior::{ai},
        abilities: &[{", ".join(abilities)}],
        difficulty: {m['difficulty']},
        frequency: {m.get('frequency', 0)},
        gen_flags: &[{", ".join(rs(g) for g in m.get('gen', []))}],
        flags: &[{", ".join(rs(g) for g in m.get('flags1', []) + m.get('flags2', []) + m.get('flags3', []))}],
    }},
"""


def id_table(ty, doc, ids):
    """Index newtype constants named exactly as the C enum, plus the C names by index."""
    return (f"c_id!({ty}, {len(ids)});\n\n"
            f"/// {doc}\n#[allow(missing_docs)]\nimpl {ty} {{\n"
            + "".join(f"    pub const {c}: Self = Self({i});\n" for i, c in enumerate(ids))
            + "    /// C enum name of every id, by index.\n"
            "    pub const C_NAMES: &'static [&'static str] = &[\n"
            + "".join(f"        {rs(c)},\n" for c in ids) + "    ];\n}\n\n")


def render_monsters(mons):
    names = monster_names(mons)
    return (HEADER + id_table("MonsterSpeciesId", "Monster species ids: C `PM_*` names (prefix dropped), `mons[]` order.",
                              [m["id"] for m in mons])
            + "/// The bestiary, generated from `data/nethack-5.0/monsters.toml`; index = id.\n"
            "pub static BESTIARY: &[MonsterArchetype] = &[\n"
            + "".join(monster_entry(m, names[m["id"]]) for m in mons) + "];\n")


# -------------------------------------------------------------------- items

CLASS = {"weapon": "Weapon", "armor": "Armor", "ring": "Ring", "amulet": "Amulet", "tool": "Tool",
         "food": "Food", "potion": "Potion", "scroll": "Scroll", "spbook": "Spellbook", "wand": "Wand",
         "coin": "Coin", "gem": "Gem", "rock": "Rock", "ball": "Ball", "chain": "Chain", "venom": "Venom"}
PREFIX = {"wand": "wand of ", "ring": "ring of ", "potion": "potion of ", "scroll": "scroll of ",
          "spbook": "spellbook of "}
NO_PREFIX = {"SPE_NOVEL", "SPE_BOOK_OF_THE_DEAD"}  # objnam.c obj_typename() exceptions
SLOT = {"suit": "Suit", "cloak": "Cloak", "helm": "Helmet", "shield": "Shield", "gloves": "Gloves",
        "boots": "Boots", "shirt": "Shirt"}
WDIR = {"nodir": "NoDir", "immediate": "Immediate", "ray": "Ray"}


def object_name(o):
    """Identified singular name, as objnam.c obj_typename() builds it."""
    prefix = "" if o["id"] in NO_PREFIX else PREFIX.get(o["class"], "")
    return prefix + o["name"]


def item_entry(variant, name, o, artifact=None):
    w = o["src"]
    cls = pick(CLASS, o["class"], w)
    flags = o.get("flags", [])
    sdam, ldam = o.get("damage_small", 0), o.get("damage_large", 0)
    armor = o["class"] == "armor"
    wdir = pick(WDIR, o["dir"][0], w) if o["class"] == "wand" and o.get("dir") else None
    if o["class"] == "wand" and wdir is None:
        die(f"{w}: wand without direction")
    src = artifact["src"] if artifact else w
    return f"""    ItemArchetype {{
        // {src}
        id: ItemKindId::{variant},
        name: {rs(name)},
        class: ItemClass::{cls},
        weight: {o['weight']},
        cost: {artifact['cost'] if artifact else o['cost']},
        damage_small: ({1 if sdam else 0}, {sdam}),
        damage_large: ({1 if ldam else 0}, {ldam}),
        ac_bonus: {o['oc1'] if armor else 0},
        is_container: {str('container' in flags).lower()},
        is_bag_of_holding: {str(o['id'] == 'BAG_OF_HOLDING').lower()},
        oc_magic: {str('magic' in flags or artifact is not None).lower()},
        wand_dir: {f'Some(WandDir::{wdir})' if wdir else 'None'},
        nutrition: {o['nutrition']},
        prob: {0 if artifact else o['prob']},
        material: {rs(o.get('material', 'none'))},
        magic_cancellation: {o['oc2'] if armor else 0},
        armor_slot: {f"Some(ArmorSlot::{pick(SLOT, o['armor_slot'], w)})" if armor else 'None'},
        artifact: {str(artifact is not None).lower()},
    }},
"""


def render_items(objs, arts):
    by_id = {o["id"]: o for o in objs}
    rows = [(o["id"], object_name(o), o, None) for o in objs if "name" in o]  # unnamed = extra scroll labels
    rows += [("ART_" + a["id"], a["name"], by_id[a["base"]], a) for a in arts]
    return (HEADER + id_table("ItemKindId", "Item kind ids: C `objects[]` names, then artifacts as C `ART_*`.",
                              [r[0] for r in rows])
            + "/// The item catalog, generated from `data/nethack-5.0/{objects,artifacts}.toml`; index = id.\n"
            "pub static ITEM_CATALOG: &[ItemArchetype] = &[\n"
            + "".join(item_entry(*r) for r in rows) + "];\n")


def render_class_probs(tables):
    out = ""
    for t in tables:
        rows = "".join(f"    ({e['prob']}, ItemClass::{pick(CLASS, e['class'], t['src'])}),\n" for e in t["entries"])
        out += (f"\n/// C `{t['id']}[]` ({t['src']}): `(percent, class)`, rolled with `rnd(100)`.\n"
                f"pub static {t['id'].upper()}: &[(u32, ItemClass)] = &[\n{rows}];\n")
    return out


STATS = ["str", "int", "wis", "dex", "con", "cha"]


def advance(a):
    return ("Advance { " + ", ".join(f"{k}: {a[k]}" for k in ["infix", "inrnd", "lofix", "lornd", "hifix", "hirnd"])
            + " }")


def render_roles(roles, races):
    arr = lambda d: "[" + ", ".join(str(d[k]) for k in STATS) + "]"
    out = HEADER + "\n/// C `roles[]` (src/role.c): attribute and advancement tables.\npub static ROLE_STATS: &[RoleStats] = &[\n"
    for r in roles:
        out += (f"    RoleStats {{ name: {rs(r['name'])}, attr_base: {arr(r['attr_base'])}, attr_dist: {arr(r['attr_dist'])}, "
                f"hp: {advance(r['hp_advance'])}, energy: {advance(r['energy_advance'])}, xlev: {r['xlev']}, "
                f"initial_record: {r['initial_record']} }},\n")
    out += "];\n\n/// C `races[]` (src/role.c): attribute limits and advancement.\npub static RACE_STATS: &[RaceStats] = &[\n"
    for r in races:
        out += (f"    RaceStats {{ noun: {rs(r['noun'])}, attr_min: {arr(r['attr_min'])}, attr_max: {arr(r['attr_max'])}, "
                f"hp: {advance(r['hp_advance'])}, energy: {advance(r['energy_advance'])} }},\n")
    return out + "];\n"


HEADER = ("// GENERATED by scripts/extract/gen_rust.py from data/nethack-5.0/*.toml.\n"
          "// Do not edit: change the data or the generator.\n")


def main():
    files = {
        "monsters.rs": render_monsters(load("monsters.toml", "monster")),
        "roles.rs": render_roles(load("roles.toml", "role"), load("races.toml", "race")),
        "items.rs": render_items(load("objects.toml", "object"), load("artifacts.toml", "artifact"))
        + render_class_probs(load("class_probs.toml", "class_probs")),
    }
    stale = []
    for name, text in files.items():
        p = OUT / name
        if "--check" in sys.argv:
            if not p.exists() or p.read_text() != text:
                stale.append(name)
        else:
            OUT.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
    if stale:
        die("stale: " + ", ".join(stale) + " (run scripts/extract/gen_rust.py)")
    print("generated:", ", ".join(files))


if __name__ == "__main__":
    main()
