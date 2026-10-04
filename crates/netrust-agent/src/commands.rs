//! Shared action and character parsing for all frontends.

use netrust_core::EngravingMedium;
use netrust_data::{CharacterConfig, Gender, RACES, ROLES};
use netrust_sim::{ActionAst, Alignment, Coord, Direction};
use netrust_types::WaterType;

/// Energy used by the `zap` action.
pub const ZAP_ENERGY: u32 = 6;
/// Maximum wish length in characters.
pub const MAX_WISH_LEN: usize = 128;

/// Optional arguments accompanying an action name.
#[derive(Debug, Default, Clone)]
pub struct ActionArgs {
    pub index: Option<usize>,
    pub direction: Option<String>,
    pub target: Option<(usize, usize)>,
    pub text: Option<String>,
    pub player: Option<Coord>,
}

/// Parse a compass direction name or vi-key.
pub fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_lowercase().as_str() {
        "north" | "k" => Some(Direction::North),
        "south" | "j" => Some(Direction::South),
        "east" | "l" => Some(Direction::East),
        "west" | "h" => Some(Direction::West),
        "northeast" | "u" => Some(Direction::NorthEast),
        "northwest" | "y" => Some(Direction::NorthWest),
        "southeast" | "n" => Some(Direction::SouthEast),
        "southwest" | "b" => Some(Direction::SouthWest),
        _ => None,
    }
}

fn dir_arg(s: &str) -> Result<Direction, String> {
    parse_direction(s).ok_or_else(|| format!("invalid direction '{s}'"))
}

fn required_dir(args: &ActionArgs) -> Result<Direction, String> {
    match &args.direction {
        Some(s) => dir_arg(s),
        None => Err("requires a direction".to_string()),
    }
}

fn compass_or(args: &ActionArgs, default: Direction) -> Result<Direction, String> {
    match &args.direction {
        Some(s) => dir_arg(s),
        None => Ok(default),
    }
}

fn step_from_player(args: &ActionArgs, dir: Direction) -> Result<Coord, String> {
    args.player
        .ok_or_else(|| "requires player position".to_string())?
        .step(dir)
        .ok_or_else(|| "target off the map".to_string())
}

fn target_coord(args: &ActionArgs) -> Result<Coord, String> {
    if let Some((x, y)) = args.target {
        return Coord::new(x, y).ok_or_else(|| "target off the map".to_string());
    }
    let dir = match &args.direction {
        Some(s) => dir_arg(s)?,
        None => return Err("requires a target or a direction".to_string()),
    };
    step_from_player(args, dir)
}

fn parse_dip(text: Option<&str>) -> Result<ActionAst, String> {
    let text = text.unwrap_or("0");
    let (idx, water) = match text.split_once(':') {
        Some((i, w)) => (i, w),
        None => (text, "holy"),
    };
    let item_index = idx
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("invalid dip item index '{idx}'"))?;
    let into_water = match water.trim().to_lowercase().as_str() {
        "holy" | "blessed" => WaterType::Holy,
        "unholy" | "cursed" => WaterType::Unholy,
        "plain" | "uncursed" => WaterType::Plain,
        other => return Err(format!("invalid water type '{other}'")),
    };
    Ok(ActionAst::Dip {
        item_index,
        into_water,
    })
}

/// Parse an action name plus arguments into an [`ActionAst`].
pub fn parse_action(name: &str, args: &ActionArgs) -> Result<ActionAst, String> {
    let n = name.trim().to_lowercase();
    let idx = args.index.unwrap_or(0);
    if let Some(d) = n.strip_prefix("move_") {
        return Ok(ActionAst::Move(dir_arg(d)?));
    }
    if let Some(d) = n.strip_prefix("kick_") {
        return Ok(ActionAst::Kick(step_from_player(args, dir_arg(d)?)?));
    }
    if let Some(d) = parse_direction(&n) {
        return Ok(ActionAst::Move(d));
    }
    match n.as_str() {
        "move" => Ok(ActionAst::Move(required_dir(args)?)),
        "kick" => Ok(ActionAst::Kick(target_coord(args)?)),
        "open_door" => Ok(ActionAst::OpenDoor(target_coord(args)?)),
        "close_door" => Ok(ActionAst::CloseDoor(target_coord(args)?)),
        "untrap" => Ok(ActionAst::Untrap(target_coord(args)?)),
        "drop" => Ok(ActionAst::Drop(idx)),
        "wield" => Ok(ActionAst::Wield(idx)),
        "quaff" => Ok(ActionAst::Quaff(idx)),
        "read" => Ok(ActionAst::Read(idx)),
        "eat" => Ok(ActionAst::Eat(idx)),
        "sacrifice" => Ok(ActionAst::Sacrifice(idx)),
        "rub" => Ok(ActionAst::Rub(idx)),
        "apply" | "light" => Ok(ActionAst::Apply(idx)),
        "price_check" | "appraise" => Ok(ActionAst::PriceCheck(idx)),
        "donate" => Ok(ActionAst::Donate(idx as u32)),
        "cast" => Ok(ActionAst::Cast {
            spell_index: idx,
            dir: compass_or(args, Direction::East)?,
        }),
        "zap" => Ok(ActionAst::ZapWand {
            dir: compass_or(args, Direction::East)?,
            energy: ZAP_ENERGY,
        }),
        "fire" => Ok(ActionAst::Fire(required_dir(args)?)),
        "wish" => {
            let text = args
                .text
                .as_ref()
                .ok_or_else(|| "wish requires text".to_string())?;
            if text.chars().count() > MAX_WISH_LEN {
                return Err(format!("wish text exceeds {MAX_WISH_LEN} characters"));
            }
            Ok(ActionAst::Wish(text.clone()))
        }
        "engrave" => Ok(ActionAst::Engrave {
            text: args.text.clone().unwrap_or_else(|| "Elbereth".to_string()),
            medium: EngravingMedium::Dust(1),
        }),
        "dip" => parse_dip(args.text.as_deref()),
        "pickup" => Ok(ActionAst::PickUp),
        "search" => Ok(ActionAst::Search),
        "pay" => Ok(ActionAst::Pay),
        "pray" => Ok(ActionAst::Pray),
        "ascend" => Ok(ActionAst::Ascend),
        "descend" => Ok(ActionAst::Descend),
        "wait" => Ok(ActionAst::Wait),
        _ => Err(format!("Unknown action '{name}'")),
    }
}

/// Build a [`CharacterConfig`] from optional string fields; `None` keeps the default.
pub fn parse_character(
    name: Option<&str>,
    role: Option<&str>,
    race: Option<&str>,
    gender: Option<&str>,
    alignment: Option<&str>,
) -> Result<CharacterConfig, String> {
    let mut c = CharacterConfig::default();
    if let Some(n) = name {
        c.name = n.to_string();
    }
    if let Some(r) = role {
        c.role = ROLES
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(r))
            .map(|s| s.id)
            .ok_or_else(|| {
                let names: Vec<&str> = ROLES.iter().map(|s| s.name).collect();
                format!("unknown role '{r}'; expected one of: {}", names.join(", "))
            })?;
    }
    if let Some(r) = race {
        c.race = RACES
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(r))
            .map(|s| s.id)
            .ok_or_else(|| {
                let names: Vec<&str> = RACES.iter().map(|s| s.name).collect();
                format!("unknown race '{r}'; expected one of: {}", names.join(", "))
            })?;
    }
    if let Some(g) = gender {
        c.gender = match g.to_lowercase().as_str() {
            "male" => Gender::Male,
            "female" => Gender::Female,
            _ => {
                return Err(format!(
                    "unknown gender '{g}'; expected one of: male, female"
                ))
            }
        };
    }
    if let Some(a) = alignment {
        c.alignment = match a.to_lowercase().as_str() {
            "lawful" => Alignment::Lawful,
            "neutral" => Alignment::Neutral,
            "chaotic" => Alignment::Chaotic,
            _ => {
                return Err(format!(
                    "unknown alignment '{a}'; expected one of: lawful, neutral, chaotic"
                ))
            }
        };
    }
    Ok(c)
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;
    use netrust_sim::{ActionAst, Coord, Direction};

    fn a() -> ActionArgs {
        ActionArgs::default()
    }
    fn with_dir(d: &str) -> ActionArgs {
        ActionArgs {
            direction: Some(d.into()),
            ..a()
        }
    }
    fn with_idx(i: usize) -> ActionArgs {
        ActionArgs {
            index: Some(i),
            ..a()
        }
    }
    fn at(x: usize, y: usize) -> ActionArgs {
        ActionArgs {
            player: Coord::new(x, y),
            ..a()
        }
    }

    #[test]
    fn moves_in_all_spellings() {
        assert_eq!(
            parse_action("move_north", &a()).unwrap(),
            ActionAst::Move(Direction::North)
        );
        assert_eq!(
            parse_action("Move_NorthEast", &a()).unwrap(),
            ActionAst::Move(Direction::NorthEast)
        );
        assert_eq!(
            parse_action("move", &with_dir("sw"))
                .unwrap_err()
                .contains("direction"),
            true
        );
        assert_eq!(
            parse_action("move", &with_dir("southwest")).unwrap(),
            ActionAst::Move(Direction::SouthWest)
        );
        assert_eq!(
            parse_action("north", &a()).unwrap(),
            ActionAst::Move(Direction::North)
        );
        assert_eq!(
            parse_action("k", &a()).unwrap(),
            ActionAst::Move(Direction::North)
        );
        assert!(parse_action("move", &a()).is_err());
    }

    #[test]
    fn indexed_actions_default_to_zero() {
        assert_eq!(
            parse_action("drop", &with_idx(3)).unwrap(),
            ActionAst::Drop(3)
        );
        assert_eq!(parse_action("quaff", &a()).unwrap(), ActionAst::Quaff(0));
        for (n, v) in [
            ("wield", ActionAst::Wield(2)),
            ("read", ActionAst::Read(2)),
            ("eat", ActionAst::Eat(2)),
            ("sacrifice", ActionAst::Sacrifice(2)),
            ("apply", ActionAst::Apply(2)),
            ("light", ActionAst::Apply(2)),
            ("price_check", ActionAst::PriceCheck(2)),
            ("appraise", ActionAst::PriceCheck(2)),
            ("rub", ActionAst::Rub(2)),
            ("donate", ActionAst::Donate(2)),
        ] {
            assert_eq!(parse_action(n, &with_idx(2)).unwrap(), v, "{n}");
        }
    }

    #[test]
    fn directional_actions() {
        assert_eq!(
            parse_action("cast", &with_dir("west")).unwrap(),
            ActionAst::Cast {
                spell_index: 0,
                dir: Direction::West
            }
        );
        assert_eq!(
            parse_action("cast", &a()).unwrap(),
            ActionAst::Cast {
                spell_index: 0,
                dir: Direction::East
            }
        );
        assert_eq!(
            parse_action("zap", &with_dir("n")).unwrap(),
            ActionAst::ZapWand {
                dir: Direction::SouthEast,
                energy: ZAP_ENERGY
            }
        );
        assert_eq!(
            parse_action("fire", &with_dir("north")).unwrap(),
            ActionAst::Fire(Direction::North)
        );
        assert!(parse_action("fire", &a()).is_err());
        assert!(parse_action("cast", &with_dir("up")).is_err());
    }

    #[test]
    fn targeted_actions() {
        let p = at(10, 10);
        assert_eq!(
            parse_action("kick_east", &p).unwrap(),
            ActionAst::Kick(Coord::new(11, 10).unwrap())
        );
        assert_eq!(
            parse_action(
                "kick",
                &ActionArgs {
                    direction: Some("north".into()),
                    ..p.clone()
                }
            )
            .unwrap(),
            ActionAst::Kick(Coord::new(10, 9).unwrap())
        );
        assert_eq!(
            parse_action(
                "open_door",
                &ActionArgs {
                    target: Some((3, 4)),
                    ..a()
                }
            )
            .unwrap(),
            ActionAst::OpenDoor(Coord::new(3, 4).unwrap())
        );
        assert_eq!(
            parse_action(
                "close_door",
                &ActionArgs {
                    direction: Some("w".into()),
                    ..p.clone()
                }
            )
            .unwrap_err()
            .contains("direction"),
            true
        );
        assert_eq!(
            parse_action(
                "untrap",
                &ActionArgs {
                    direction: Some("west".into()),
                    ..p.clone()
                }
            )
            .unwrap(),
            ActionAst::Untrap(Coord::new(9, 10).unwrap())
        );
        assert!(parse_action("kick_east", &at(79, 0)).is_err());
        assert!(parse_action(
            "open_door",
            &ActionArgs {
                target: Some((500, 4)),
                ..a()
            }
        )
        .is_err());
        assert!(
            parse_action("kick_east", &a()).is_err(),
            "needs player position"
        );
    }

    #[test]
    fn text_actions() {
        assert_eq!(
            parse_action(
                "wish",
                &ActionArgs {
                    text: Some("long sword".into()),
                    ..a()
                }
            )
            .unwrap(),
            ActionAst::Wish("long sword".into())
        );
        assert!(parse_action("wish", &a()).is_err());
        let ok = "ж".repeat(MAX_WISH_LEN);
        assert!(parse_action(
            "wish",
            &ActionArgs {
                text: Some(ok),
                ..a()
            }
        )
        .is_ok());
        let long = "ж".repeat(MAX_WISH_LEN + 1);
        assert!(parse_action(
            "wish",
            &ActionArgs {
                text: Some(long),
                ..a()
            }
        )
        .is_err());
        assert!(
            matches!(parse_action("engrave", &a()).unwrap(), ActionAst::Engrave { ref text, .. } if text == "Elbereth")
        );
        assert!(matches!(
            parse_action(
                "dip",
                &ActionArgs {
                    text: Some("2:unholy".into()),
                    ..a()
                }
            )
            .unwrap(),
            ActionAst::Dip {
                item_index: 2,
                into_water: netrust_types::WaterType::Unholy
            }
        ));
    }

    #[test]
    fn simple_and_unknown() {
        for (n, v) in [
            ("pickup", ActionAst::PickUp),
            ("search", ActionAst::Search),
            ("pay", ActionAst::Pay),
            ("pray", ActionAst::Pray),
            ("ascend", ActionAst::Ascend),
            ("descend", ActionAst::Descend),
            ("wait", ActionAst::Wait),
        ] {
            assert_eq!(parse_action(n, &a()).unwrap(), v);
        }
        assert!(parse_action("dance", &a()).is_err());
        assert!(parse_action("", &a()).is_err());
    }

    #[test]
    fn every_legacy_name_still_parses() {
        // Names accepted by MCP STEP_ACTIONS, JSON-RPC netrust.step, bin/jsonrpc.rs, GraphQL stepAction, WASM step
        let p = at(10, 10);
        let legacy = [
            "move_north",
            "move_east",
            "move_south",
            "move_west",
            "move_northeast",
            "move_northwest",
            "move_southeast",
            "move_southwest",
            "wait",
            "pickup",
            "pay",
            "pray",
            "sacrifice",
            "eat",
            "cast",
            "ascend",
            "descend",
            "kick_north",
            "kick_east",
            "kick_south",
            "kick_west",
            "north",
            "east",
            "south",
            "west",
            "k",
            "l",
            "j",
            "h",
            "drop",
            "wield",
            "quaff",
            "read",
            "rub",
            "price_check",
            "appraise",
            "engrave",
            "apply",
            "light",
            "donate",
        ];
        for n in legacy {
            assert!(parse_action(n, &p).is_ok(), "legacy name {n} rejected");
        }
        assert!(parse_action(
            "move",
            &ActionArgs {
                direction: Some("north".into()),
                ..p.clone()
            }
        )
        .is_ok());
        assert!(parse_action(
            "open_door",
            &ActionArgs {
                target: Some((10, 9)),
                ..p.clone()
            }
        )
        .is_ok());
        assert!(parse_action("zap", &p).is_ok());
        assert!(parse_action(
            "wish",
            &ActionArgs {
                text: Some("x".into()),
                ..p
            }
        )
        .is_ok());
    }

    #[test]
    fn character_parsing() {
        let c = parse_character(None, None, None, None, None).unwrap();
        assert_eq!(c.name, "Hero");
        let c = parse_character(
            Some("Ann"),
            Some("WIZARD"),
            Some("elf"),
            Some("male"),
            Some("chaotic"),
        )
        .unwrap();
        assert_eq!(c.role, netrust_data::RoleId::Wizard);
        assert_eq!(c.race, netrust_data::RaceId::Elf);
        assert_eq!(c.gender, netrust_data::Gender::Male);
        assert_eq!(c.alignment, netrust_types::Alignment::Chaotic);
        assert!(parse_character(None, Some("samurai"), None, None, None)
            .unwrap_err()
            .contains("samurai"));
        assert!(parse_character(None, None, Some("hobbit"), None, None).is_err());
        assert!(parse_character(None, None, None, Some("other"), None).is_err());
        assert!(parse_character(None, None, None, None, Some("evil")).is_err());
    }
}
