//! NetHackED Terminal User Interface (TUI).
//!
//! Classic ASCII 80x21 NetHack presentation layer running directly on Crossterm.
//! Fully supports canonical subsystems: 12 Traps, #enhance Skill Tree,
//! #conduct Voluntary Conduct Tracker, Quivers & Ranged Firing, Steeds/Riding,
//! Status Afflictions (Petrification, Sliming, Polymorph), and screen-centered layouts.

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use nethacked_agent::commands::ZAP_ENERGY;
use nethacked_core::skills::{enhance_skill, skill_damage_bonus, skill_to_hit_bonus};
use nethacked_dungeon::compute_fov;
use nethacked_i18n::{t, t_align, t_hunger_str, t_item, Locale, Messages};
use nethacked_sim::{
    ActionAst, Alignment, CharacterConfig, Coord, Direction, DoorState, GameEvent, Gender,
    HungerState, RaceId, RoleId, SimulationWorld, Tile, COLNO, ROWNO,
};
use nethacked_types::{ItemId, SkillClass, SkillLevel, TrapState};
mod keys;
mod pager;
use keys::{
    confirm_quit_answer, handle_key, is_cancel, map_ukrainian_key, InventoryPurpose, KeyContext,
    KeyOutcome, HELP_KEYS,
};
use pager::Pager;
use std::collections::HashSet;
use std::io::{self, stdout, Stdout, Write};

fn role_ui_keys(role_id: RoleId) -> (char, &'static str, &'static str, &'static str) {
    match role_id {
        RoleId::Valkyrie => (
            'v',
            "role.valkyrie",
            "role.valkyrie.race",
            "role.valkyrie.desc",
        ),
        RoleId::Wizard => ('w', "role.wizard", "role.wizard.race", "role.wizard.desc"),
        RoleId::Barbarian => (
            'b',
            "role.barbarian",
            "role.barbarian.race",
            "role.barbarian.desc",
        ),
        RoleId::Rogue => ('r', "role.rogue", "role.rogue.race", "role.rogue.desc"),
        RoleId::Knight => ('k', "role.knight", "role.knight.race", "role.knight.desc"),
        RoleId::Monk => ('m', "role.monk", "role.monk.race", "role.monk.desc"),
        RoleId::Healer => ('h', "role.healer", "role.healer.race", "role.healer.desc"),
        RoleId::Tourist => (
            't',
            "role.tourist",
            "role.tourist.race",
            "role.tourist.desc",
        ),
        RoleId::Archaeologist => (
            'a',
            "role.archaeologist",
            "role.archaeologist.race",
            "role.archaeologist.desc",
        ),
    }
}

struct TerminalGuard;

impl TerminalGuard {
    pub fn new(stdout: &mut Stdout) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// Calculate horizontal and vertical offsets to center an 80x24 NetHack viewport on any screen size.
fn screen_offsets() -> (u16, u16) {
    let (term_width, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
    let offset_x = (term_width.saturating_sub(80)) / 2;
    let offset_y = (term_height.saturating_sub(24)) / 2;
    (offset_x, offset_y)
}

fn select_character(
    stdout: &mut Stdout,
    locale: Locale,
    ruleset: &nethacked_data::ruleset::Ruleset,
) -> io::Result<Option<CharacterConfig>> {
    let (ox, oy) = screen_offsets();
    execute!(stdout, Clear(ClearType::All))?;

    let border_top =
        "+------------------------------------------------------------------------------+";
    let title_line = format!(
        "|{:^78}|",
        "NetHackED: Classic NetHack 5.0 in Rust & Lean 4"
    );
    let border_mid = format!("+{:-^78}+", " from Yemelianov (Emelyanov Dmytro) ");
    let border_bot =
        "+------------------------------------------------------------------------------+";

    execute!(
        stdout,
        MoveTo(ox, oy),
        SetForegroundColor(Color::Cyan),
        Print(border_top),
        MoveTo(ox, oy + 1),
        SetForegroundColor(Color::Yellow),
        Print(title_line),
        MoveTo(ox, oy + 2),
        SetForegroundColor(Color::Cyan),
        Print(border_mid),
        ResetColor
    )?;

    let header_desc = t("tui.pick_role", locale);

    execute!(
        stdout,
        MoveTo(ox + 2, oy + 4),
        SetForegroundColor(Color::White),
        Print(header_desc),
        ResetColor
    )?;

    let roles: Vec<(String, &str, &str)> = ruleset
        .roles
        .iter()
        .map(|r| {
            let (hotkey, key, race_key, desc_key) = role_ui_keys(r.id);
            (
                format!("[{hotkey}] {}", t(key, locale)),
                t(race_key, locale),
                t(desc_key, locale),
            )
        })
        .collect();

    for (idx, (role_name, align_race, details)) in roles.iter().enumerate() {
        let line = format!("    {role_name:<18} - {align_race:<18} {details}");
        execute!(
            stdout,
            MoveTo(ox, oy + 6 + idx as u16),
            SetForegroundColor(Color::White),
            Print(line),
            ResetColor
        )?;
    }

    let prompt = t("tui.role_prompt", locale);

    execute!(
        stdout,
        MoveTo(ox + 2, oy + 17),
        SetForegroundColor(Color::Green),
        Print(prompt),
        MoveTo(ox, oy + 19),
        SetForegroundColor(Color::Cyan),
        Print(border_bot),
        ResetColor
    )?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };
            match code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                KeyCode::Char('v') | KeyCode::Enter | KeyCode::Char(' ') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.valkyrie", locale).into(),
                        role: RoleId::Valkyrie,
                        race: RaceId::Human,
                        gender: Gender::Female,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('w') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.wizard", locale).into(),
                        role: RoleId::Wizard,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('b') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.barbarian", locale).into(),
                        role: RoleId::Barbarian,
                        race: RaceId::Orc,
                        gender: Gender::Male,
                        alignment: Alignment::Chaotic,
                    }));
                }
                KeyCode::Char('r') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.rogue", locale).into(),
                        role: RoleId::Rogue,
                        race: RaceId::Human,
                        gender: Gender::Female,
                        alignment: Alignment::Chaotic,
                    }));
                }
                KeyCode::Char('k') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.knight", locale).into(),
                        role: RoleId::Knight,
                        race: RaceId::Dwarf,
                        gender: Gender::Male,
                        alignment: Alignment::Lawful,
                    }));
                }
                KeyCode::Char('m') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.monk", locale).into(),
                        role: RoleId::Monk,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('h') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.healer", locale).into(),
                        role: RoleId::Healer,
                        race: RaceId::Gnome,
                        gender: Gender::Female,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('t') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.tourist", locale).into(),
                        role: RoleId::Tourist,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('a') => {
                    return Ok(Some(CharacterConfig {
                        name: t("role.archaeologist", locale).into(),
                        role: RoleId::Archaeologist,
                        race: RaceId::Human,
                        gender: Gender::Female,
                        alignment: Alignment::Lawful,
                    }));
                }
                _ => {}
            }
        }
    }
}

/// First adjacent door in the given state, scanning compass directions.
fn adjacent_door(world: &SimulationWorld, from: Coord, want: DoorState) -> Option<Coord> {
    Direction::all_compass().iter().find_map(|&d| {
        from.step(d).filter(
            |&c| matches!(world.level.get_tile(c), Tile::Door { state, .. } if *state == want),
        )
    })
}

fn prompt_direction(stdout: &mut Stdout, prompt_msg: &str) -> io::Result<Option<Direction>> {
    let (ox, oy) = screen_offsets();
    execute!(
        stdout,
        MoveTo(ox, oy),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(Color::Yellow),
        Print(prompt_msg),
        ResetColor
    )?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_cancel(&key) {
                return Ok(None);
            }
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };
            match code {
                KeyCode::Char('h') | KeyCode::Left => return Ok(Some(Direction::West)),
                KeyCode::Char('l') | KeyCode::Right => return Ok(Some(Direction::East)),
                KeyCode::Char('k') | KeyCode::Up => return Ok(Some(Direction::North)),
                KeyCode::Char('j') | KeyCode::Down => return Ok(Some(Direction::South)),
                KeyCode::Char('y') => return Ok(Some(Direction::NorthWest)),
                KeyCode::Char('u') => return Ok(Some(Direction::NorthEast)),
                KeyCode::Char('b') => return Ok(Some(Direction::SouthWest)),
                KeyCode::Char('n') => return Ok(Some(Direction::SouthEast)),
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => return Ok(None),
                _ => {}
            }
        }
    }
}

fn show_inventory_modal(
    stdout: &mut Stdout,
    world: &SimulationWorld,
) -> io::Result<Option<ItemId>> {
    let loc = world.locale;
    let carried = world.arena.items_carried_by(world.player_id);
    let mut pager = Pager::default();

    loop {
        let (ox, oy) = screen_offsets();
        queue!(stdout, Clear(ClearType::All))?;
        let title = t("tui.inventory_title", loc);
        queue!(
            stdout,
            MoveTo(ox + 2, oy + 1),
            SetForegroundColor(Color::Cyan),
            Print(title),
            ResetColor
        )?;

        if carried.is_empty() {
            let empty_msg = t("tui.pack_empty", loc);
            queue!(
                stdout,
                MoveTo(ox + 4, oy + 3),
                SetForegroundColor(Color::DarkGrey),
                Print(empty_msg),
                ResetColor
            )?;
        } else {
            for (idx, &item_id) in pager.page_items(&carried).iter().enumerate() {
                let letter = (b'a' + idx as u8) as char;
                if let Some(item) = world.arena.items.get(item_id) {
                    let equipped_tag = if world.wielded_item == Some(item_id) {
                        t("tui.in_hand", loc)
                    } else {
                        ""
                    };
                    let name = t_item(&item.name, loc);
                    let weight_label = t("tui.weight", loc);
                    let desc = format!(
                        "  [{}] {} - {}: {}{}",
                        letter, name, weight_label, item.weight, equipped_tag
                    );
                    queue!(
                        stdout,
                        MoveTo(ox + 2, oy + 3 + idx as u16),
                        SetForegroundColor(Color::White),
                        Print(desc),
                        ResetColor
                    )?;
                }
            }
            let pages = Pager::page_count(carried.len());
            if pages > 1 {
                let footer = Messages::inventory_page_footer(pager.page + 1, pages, loc);
                queue!(
                    stdout,
                    MoveTo(ox + 2, oy + 23),
                    SetForegroundColor(Color::DarkGrey),
                    Print(footer),
                    ResetColor
                )?;
            }
        }
        stdout.flush()?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if is_cancel(&key) {
                    return Ok(None);
                }
                let code = match key.code {
                    KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                    other => other,
                };
                match code {
                    KeyCode::Esc | KeyCode::Char(' ') => return Ok(None),
                    KeyCode::Char('>') | KeyCode::Char('.') | KeyCode::PageDown => {
                        pager.next(carried.len())
                    }
                    KeyCode::Char('<') | KeyCode::Char(',') | KeyCode::PageUp => pager.prev(),
                    KeyCode::Char(c) => {
                        if let Some(idx) = pager.select(c, carried.len()) {
                            return Ok(Some(carried[idx]));
                        }
                    }
                    _ => {}
                }
            }
            // Resize (or any other event) just redraws on the next loop turn.
            _ => {}
        }
    }
}

fn show_conducts_modal(stdout: &mut Stdout, world: &SimulationWorld) -> io::Result<()> {
    let (ox, oy) = screen_offsets();
    execute!(stdout, Clear(ClearType::All))?;
    let loc = world.locale;
    let title = t("tui.conducts_title", loc);

    execute!(
        stdout,
        MoveTo(ox + 2, oy + 1),
        SetForegroundColor(Color::Yellow),
        Print(title),
        ResetColor
    )?;

    let conducts = [
        (t("conduct.pacifist", loc), world.conducts.pacifist),
        (t("conduct.vegan", loc), world.conducts.vegan),
        (t("conduct.vegetarian", loc), world.conducts.vegetarian),
        (t("conduct.atheist", loc), world.conducts.atheist),
        (t("conduct.illiterate", loc), world.conducts.illiterate),
        (t("conduct.genocideless", loc), world.conducts.genocideless),
        (t("conduct.polypileless", loc), world.conducts.polypileless),
        (t("conduct.wishless", loc), world.conducts.wishless),
    ];

    for (idx, (name, active)) in conducts.iter().enumerate() {
        let (status_str, status_color) = if *active {
            (t("tui.conduct_active", loc), Color::Green)
        } else {
            (t("tui.conduct_broken", loc), Color::Red)
        };

        execute!(
            stdout,
            MoveTo(ox + 4, oy + 3 + (idx * 2) as u16),
            SetForegroundColor(Color::White),
            Print(format!("{name:<60} ")),
            SetForegroundColor(status_color),
            Print(status_str),
            ResetColor
        )?;
    }

    let footer = t("tui.return_prompt", loc);
    execute!(
        stdout,
        MoveTo(ox + 4, oy + 20),
        SetForegroundColor(Color::DarkGrey),
        Print(footer),
        ResetColor
    )?;

    loop {
        if let Event::Key(key) = event::read()? {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };
            if key.kind == KeyEventKind::Press
                && (is_cancel(&key)
                    || matches!(code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ')))
            {
                break;
            }
        }
    }
    Ok(())
}

fn show_enhance_modal(stdout: &mut Stdout, world: &mut SimulationWorld) -> io::Result<()> {
    loop {
        let (ox, oy) = screen_offsets();
        execute!(stdout, Clear(ClearType::All))?;
        let loc = world.locale;
        let title = Messages::enhance_title(world.hero.skills.available_slots, loc);

        execute!(
            stdout,
            MoveTo(ox + 2, oy + 1),
            SetForegroundColor(Color::Cyan),
            Print(title),
            ResetColor
        )?;

        let all_skills = [
            (SkillClass::Dagger, "Dagger"),
            (SkillClass::ShortSword, "Short Sword"),
            (SkillClass::LongSword, "Long Sword"),
            (SkillClass::Bow, "Bow"),
            (SkillClass::Crossbow, "Crossbow"),
            (SkillClass::Club, "Club"),
            (SkillClass::BareHanded, "Bare Handed"),
        ];

        for (idx, (skill_cls, name)) in all_skills.iter().enumerate() {
            let letter = (b'a' + idx as u8) as char;
            let current_lvl = world
                .hero
                .skills
                .skills
                .get(skill_cls)
                .copied()
                .unwrap_or(SkillLevel::Unskilled);
            let to_hit = skill_to_hit_bonus(current_lvl);
            let dmg = skill_damage_bonus(current_lvl);
            let lvl_name = format!("{current_lvl:?}");

            let desc = format!(
                "  [{letter}] {name:<14} : {lvl_name:<9} (To-Hit: {to_hit:+2}, Dmg: {dmg:+2})"
            );
            execute!(
                stdout,
                MoveTo(ox + 2, oy + 3 + idx as u16),
                SetForegroundColor(Color::White),
                Print(desc),
                ResetColor
            )?;
        }

        let prompt = t("tui.enhance_prompt", loc);
        execute!(
            stdout,
            MoveTo(ox + 2, oy + 14),
            SetForegroundColor(Color::Yellow),
            Print(prompt),
            ResetColor
        )?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if is_cancel(&key) {
                break;
            }
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };
            match code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => break,
                KeyCode::Char(c) if ('a'..='g').contains(&c) => {
                    let idx = (c as u8 - b'a') as usize;
                    if let Some((skill_cls, _)) = all_skills.get(idx) {
                        let _ = enhance_skill(&mut world.hero.skills, *skill_cls);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn show_help_modal(stdout: &mut Stdout, locale: Locale, seed: u64) -> io::Result<()> {
    let (ox, oy) = screen_offsets();
    execute!(stdout, Clear(ClearType::All))?;

    let title = t("tui.help_title", locale);
    execute!(
        stdout,
        MoveTo(ox + 2, oy + 1),
        SetForegroundColor(Color::Cyan),
        Print(title),
        ResetColor
    )?;

    // Three columns rendered from the single-source key list.
    const ROWS: usize = 14;
    for (idx, (c, key)) in HELP_KEYS.iter().enumerate() {
        let desc = t(key, locale);
        let (col, row) = (idx / ROWS, idx % ROWS);
        execute!(
            stdout,
            MoveTo(ox + 1 + col as u16 * 26, oy + 3 + row as u16),
            SetForegroundColor(Color::White),
            Print(format!("{c} {desc}")),
            ResetColor
        )?;
    }
    let quit_line = t("tui.help_quit", locale);
    execute!(
        stdout,
        MoveTo(ox + 1, oy + 3 + ROWS as u16 + 1),
        SetForegroundColor(Color::Yellow),
        Print(quit_line),
        ResetColor
    )?;

    let footer = t("tui.return_prompt", locale);
    execute!(
        stdout,
        MoveTo(ox + 4, oy + 21),
        SetForegroundColor(Color::DarkGrey),
        Print(footer),
        ResetColor
    )?;

    queue!(
        stdout,
        MoveTo(ox + 4, oy + 22),
        SetForegroundColor(Color::DarkGrey),
        Print(format!("{}: {}", t("tui.seed", locale), seed)),
        ResetColor
    )?;
    stdout.flush()?;

    loop {
        if let Event::Key(key) = event::read()? {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };
            if key.kind == KeyEventKind::Press
                && (is_cancel(&key)
                    || matches!(
                        code,
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') | KeyCode::Char('?')
                    ))
            {
                break;
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let tui_args = match parse_args(&args) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(1);
        }
    };
    let seed = tui_args.seed;

    let (ruleset, ruleset_ref) = if let Some(ref path) = tui_args.pack_path {
        if path.is_dir() {
            match nethacked_pack::build(path) {
                Ok((nhpack, _report)) => {
                    let mut ruleset = nhpack.ruleset;
                    ruleset.reindex();
                    let ruleset_ref = nethacked_data::ruleset::RulesetRef {
                        id: nhpack.manifest.id,
                        version: nhpack.manifest.version,
                        hash: nhpack.hash,
                    };
                    (std::sync::Arc::new(ruleset), ruleset_ref)
                }
                Err(e) => {
                    eprintln!("error building pack {}: {e}", path.display());
                    std::process::exit(1);
                }
            }
        } else {
            match nethacked_pack::load_nhpack(path) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("error loading pack {}: {e}", path.display());
                    std::process::exit(1);
                }
            }
        }
    } else {
        (
            nethacked_data::ruleset::Ruleset::vanilla(),
            nethacked_data::ruleset::RulesetRef::vanilla(),
        )
    };

    // Installed before the guard so a panic anywhere restores the terminal first.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        prev_hook(info);
    }));

    let mut stdout = stdout();
    let _guard = TerminalGuard::new(&mut stdout)?;

    let config = match select_character(&mut stdout, tui_args.locale, &ruleset)? {
        Some(cfg) => cfg,
        None => return Ok(()),
    };
    execute!(stdout, Clear(ClearType::All))?;

    let char_name = config.name.clone();
    let mut world = SimulationWorld::new_with_character_and_ruleset(
        tui_args.seed,
        config,
        ruleset,
        ruleset_ref,
    );
    world.set_locale(tui_args.locale);
    let mut message = Messages::tui_welcome(&char_name, tui_args.locale);
    let mut last_dir = Direction::East;

    loop {
        render(&mut stdout, &world, &message, tui_args.seed)?;

        let ev = event::read()?;
        if let Event::Resize(..) = ev {
            // Redraw from scratch at the new size (offsets are recomputed in render).
            execute!(stdout, Clear(ClearType::All))?;
            continue;
        }
        if let Event::Key(key) = ev {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                other => other,
            };

            let player_opt = world.arena.actors.get(world.player_id).cloned();
            let Some(player) = player_opt else {
                break;
            };
            if player.is_dead {
                message = t("tui.died", world.locale).into();
                if matches!(
                    handle_key(
                        key,
                        &KeyContext {
                            player: player.coord,
                            last_dir,
                        }
                    ),
                    KeyOutcome::ConfirmQuit
                ) || code == KeyCode::Char('q')
                {
                    break;
                }
                continue;
            }

            let outcome = handle_key(
                key,
                &KeyContext {
                    player: player.coord,
                    last_dir,
                },
            );
            let action =
                match outcome {
                    KeyOutcome::Quit => break,
                    KeyOutcome::ConfirmQuit => {
                        let (ox, oy) = screen_offsets();
                        execute!(
                            stdout,
                            MoveTo(ox, oy),
                            Clear(ClearType::CurrentLine),
                            SetForegroundColor(Color::Yellow),
                            Print(t("tui.quit_prompt", world.locale)),
                            ResetColor
                        )?;
                        let quit = loop {
                            if let Event::Key(ans) = event::read()? {
                                if ans.kind == KeyEventKind::Press {
                                    break confirm_quit_answer(ans);
                                }
                            }
                        };
                        if quit {
                            break;
                        }
                        message = String::new();
                        None
                    }
                    KeyOutcome::OpenHelp => {
                        show_help_modal(&mut stdout, world.locale, seed)?;
                        None
                    }
                    KeyOutcome::ToggleLanguage => {
                        let new_loc = if world.locale == Locale::Uk {
                            Locale::En
                        } else {
                            Locale::Uk
                        };
                        world.set_locale(new_loc);
                        message = t("tui.lang_switched", new_loc).into();
                        None
                    }
                    KeyOutcome::Act(act) => {
                        if let ActionAst::Move(d) = &act {
                            last_dir = *d;
                        }
                        Some(act)
                    }
                    KeyOutcome::ViewInventory => {
                        let _ = show_inventory_modal(&mut stdout, &world)?;
                        None
                    }
                    KeyOutcome::ExtendedCommand => {
                        let (ox, oy) = screen_offsets();
                        execute!(
                            stdout,
                            MoveTo(ox, oy),
                            Clear(ClearType::CurrentLine),
                            SetForegroundColor(Color::Yellow),
                            Print(t("tui.ext_prompt", world.locale)),
                            ResetColor
                        )?;
                        if let Event::Key(ext_key) = event::read()? {
                            if ext_key.kind == KeyEventKind::Press && !is_cancel(&ext_key) {
                                let ext_code = match ext_key.code {
                                    KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
                                    other => other,
                                };
                                match ext_code {
                                    KeyCode::Char('e') => {
                                        show_enhance_modal(&mut stdout, &mut world)?;
                                    }
                                    KeyCode::Char('c') => {
                                        show_conducts_modal(&mut stdout, &world)?;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        None
                    }
                    KeyOutcome::OpenEnhance => {
                        show_enhance_modal(&mut stdout, &mut world)?;
                        None
                    }
                    KeyOutcome::OpenConducts => {
                        show_conducts_modal(&mut stdout, &world)?;
                        None
                    }
                    KeyOutcome::PromptFire => {
                        let prompt_text = t("tui.fire_prompt", world.locale);
                        prompt_direction(&mut stdout, prompt_text)?.map(ActionAst::Fire)
                    }
                    KeyOutcome::OpenInventory(InventoryPurpose::Quiver) => {
                        if let Some(item_id) = show_inventory_modal(&mut stdout, &world)? {
                            world.hero.quivered_item = Some(item_id);
                            let item_name = world
                                .arena
                                .items
                                .get(item_id)
                                .map(|i| i.name.as_str())
                                .unwrap_or("item");
                            message = Messages::quiver_success(item_name, world.locale);
                        }
                        None
                    }
                    KeyOutcome::OpenInventory(InventoryPurpose::Quaff) => {
                        // The action takes an index into the carried-items list.
                        show_inventory_modal(&mut stdout, &world)?.and_then(|item_id| {
                            world
                                .arena
                                .items_carried_by(world.player_id)
                                .iter()
                                .position(|&i| i == item_id)
                                .map(ActionAst::Quaff)
                        })
                    }
                    KeyOutcome::OpenInventory(_) => None,
                    KeyOutcome::ToggleMount => {
                        if world.hero.mount.is_some() {
                            Some(ActionAst::Dismount)
                        } else {
                            let adj_steed = Direction::all_compass().iter().find_map(|&d| {
                                player.coord.step(d).and_then(|c| world.actor_at(c))
                            });
                            adj_steed.map(ActionAst::Mount)
                        }
                    }
                    KeyOutcome::PromptZap => {
                        let prompt_text = t("tui.zap_prompt", world.locale);
                        let dir = prompt_direction(&mut stdout, prompt_text)?.unwrap_or(last_dir);
                        Some(ActionAst::ZapWand {
                            dir,
                            energy: ZAP_ENERGY,
                        })
                    }
                    KeyOutcome::OpenDoor => adjacent_door(&world, player.coord, DoorState::Closed)
                        .map(ActionAst::OpenDoor),
                    KeyOutcome::CloseDoor => adjacent_door(&world, player.coord, DoorState::Open)
                        .map(ActionAst::CloseDoor),
                    KeyOutcome::Redraw | KeyOutcome::Nothing => None,
                };

            if let Some(act) = action {
                let events = world.step_player_action(act);
                let loc = world.locale;
                if let Some(last_msg) = events
                    .iter()
                    .filter_map(|e| match e {
                        GameEvent::LogMessage { text } => Some(text.clone()),
                        GameEvent::AttackLanded { damage, lethal, .. } => {
                            Some(Messages::tui_hit(*damage, *lethal, loc))
                        }
                        GameEvent::AttackMissed { .. } => Some(t("tui.miss", loc).into()),
                        GameEvent::DoorToggled { new_state, .. } => {
                            Some(Messages::door_state(&format!("{new_state:?}"), loc))
                        }
                        GameEvent::LevelChanged { to_depth, .. } => {
                            Some(Messages::level_enter(to_depth, loc))
                        }
                        _ => None,
                    })
                    .next_back()
                {
                    message = last_msg;
                }
            }
        }
    }

    Ok(())
}

fn render(
    stdout: &mut Stdout,
    world: &SimulationWorld,
    message: &str,
    seed: u64,
) -> io::Result<()> {
    let (ox, oy) = screen_offsets();

    let p_coord = world
        .arena
        .actors
        .get(world.player_id)
        .map(|p| p.coord)
        .unwrap_or(Coord::new_unchecked(0, 0));

    let visible: HashSet<Coord> = compute_fov(&world.level, p_coord, 8);

    // Line 0: full-width message banner; the seed tag is added only if it fits.
    let seed_tag = format!("{}:{}", t("tui.seed", world.locale), seed);
    let banner = banner_line(message, &seed_tag);
    queue!(
        stdout,
        MoveTo(ox, oy),
        SetForegroundColor(Color::Yellow),
        Print(banner),
        ResetColor
    )?;

    // Lines 1..=21: 80x21 Dungeon grid
    for y in 0..ROWNO {
        queue!(stdout, MoveTo(ox, oy + (y + 1) as u16))?;
        for x in 0..COLNO {
            let c = Coord::new_unchecked(x, y);
            if !visible.contains(&c) {
                queue!(stdout, Print(" "))?;
                continue;
            }

            // Actor priority
            if let Some(actor_id) = world.actor_at(c) {
                if actor_id == world.player_id {
                    let hero_sym = if world.hero.polymorph.is_some() {
                        "D"
                    } else {
                        "@"
                    };
                    queue!(
                        stdout,
                        SetForegroundColor(Color::White),
                        Print(hero_sym),
                        ResetColor
                    )?;
                    continue;
                } else if let Some(actor) = world.arena.actors.get(actor_id) {
                    let ch = actor
                        .name
                        .chars()
                        .next()
                        .unwrap_or('m')
                        .to_ascii_lowercase();
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Red),
                        Print(ch),
                        ResetColor
                    )?;
                    continue;
                }
            }

            // Trap priority
            if let Some(trap) = world.level.traps.get(&c) {
                if trap.state == TrapState::Revealed {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Cyan),
                        Print("^"),
                        ResetColor
                    )?;
                    continue;
                }
            }

            // Floor items priority
            let floor_items = world.arena.items_at_floor(c);
            if let Some(&item_id) = floor_items.first() {
                if let Some(item) = world.arena.items.get(item_id) {
                    let sym = item.class.symbol();
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Cyan),
                        Print(sym),
                        ResetColor
                    )?;
                    continue;
                }
            }

            // Tile rendering
            match world.level.get_tile(c) {
                Tile::Stone => queue!(stdout, Print(" "))?,
                Tile::Wall { horizontal } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::DarkGrey),
                        Print(if *horizontal { '-' } else { '|' }),
                        ResetColor
                    )?;
                }
                Tile::Room => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Grey),
                        Print("."),
                        ResetColor
                    )?;
                }
                Tile::Corr => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::DarkGrey),
                        Print("#"),
                        ResetColor
                    )?;
                }
                Tile::Door { state, .. } => {
                    let sym = match state {
                        DoorState::Open => '/',
                        DoorState::Closed | DoorState::Locked => '+',
                        DoorState::Broken => '*',
                    };
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Yellow),
                        Print(sym),
                        ResetColor
                    )?;
                }
                Tile::SecretDoor { .. } => queue!(stdout, Print(" "))?,
                Tile::Stairs { up } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Magenta),
                        Print(if *up { '<' } else { '>' }),
                        ResetColor
                    )?;
                }
                Tile::BranchStairs { up, .. } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Cyan),
                        Print(if *up { '<' } else { '>' }),
                        ResetColor
                    )?;
                }
                Tile::Pit { filled } => {
                    let (sym, col) = if *filled {
                        ('.', Color::Grey)
                    } else {
                        ('0', Color::DarkYellow)
                    };
                    queue!(stdout, SetForegroundColor(col), Print(sym), ResetColor)?;
                }
                Tile::Altar { .. } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::White),
                        Print("_"),
                        ResetColor
                    )?;
                }
                Tile::HighAltar { .. } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Yellow),
                        Print("_"),
                        ResetColor
                    )?;
                }
                Tile::Drawbridge { open } => {
                    if *open {
                        queue!(
                            stdout,
                            SetForegroundColor(Color::DarkGrey),
                            Print("."),
                            ResetColor
                        )?;
                    } else {
                        queue!(
                            stdout,
                            SetForegroundColor(Color::DarkYellow),
                            Print("#"),
                            ResetColor
                        )?;
                    }
                }
                Tile::Moat => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Cyan),
                        Print("}"),
                        ResetColor
                    )?;
                }
                Tile::Pool { frozen } => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Blue),
                        Print(if *frozen { '=' } else { '}' }),
                        ResetColor
                    )?;
                }
                Tile::Lava => {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Red),
                        Print("^"),
                        ResetColor
                    )?;
                }
            }
        }
    }

    // Line 22: Botl Status Line + Afflictions
    let locale = world.locale;
    let player = world.arena.actors.get(world.player_id);
    let (hp, max_hp, ac, _lvl, name, align_enum) = player
        .map(|p| (p.hp, p.max_hp, p.ac, p.level, p.name.as_str(), p.alignment))
        .unwrap_or((0, 0, 10, 1, "Hero", Alignment::Neutral));

    let align_str = t_align(align_enum, locale);
    let align_short: String = align_str.chars().take(3).collect();
    let hunger_code = match world.hunger_state() {
        HungerState::Satiated => "satiated",
        HungerState::Normal => "",
        HungerState::Hungry => "hungry",
        HungerState::Weak => "weak",
        HungerState::Fainting => "fainting",
        HungerState::Starved => "starved",
    };
    let hunger_display = if hunger_code.is_empty() {
        ""
    } else {
        t_hunger_str(hunger_code, locale)
    };
    let none_str = t("tui.none", locale);

    // Affliction tags
    let mut affliction_tags = Vec::new();
    if let Some(petri) = &world.hero.afflictions.petrification {
        affliction_tags.push(format!("[STONE:{}]", petri.turns_remaining));
    }
    if let Some(slime) = &world.hero.afflictions.sliming {
        affliction_tags.push(format!("[SLIME:{}]", slime.turns_remaining));
    }
    if world.hero.polymorph.is_some() {
        affliction_tags.push("[Poly]".into());
    }
    if world.hero.mount.is_some() {
        affliction_tags.push("[Mounted]".into());
    }
    let aff_str = if affliction_tags.is_empty() {
        String::new()
    } else {
        format!(" {}", affliction_tags.join(" "))
    };

    let pack_suffix = if world.ruleset_ref.id != "vanilla" {
        format!(" [{}]", world.ruleset.manifest.name)
    } else {
        String::new()
    };

    let status = format!(
        "{}:{} {}:{:<2} {}:{} {}:{}({}) {}:{}({}) {}:{:<2} {:<6} T:{:<4} {}:{}{}{}",
        name,
        align_short,
        t("dlvl", locale),
        world.depth,
        t("gold", locale),
        world.player_gold,
        t("hp", locale),
        hp,
        max_hp,
        t("pw", locale),
        world.player_pw,
        world.player_max_pw,
        t("ac", locale),
        ac,
        hunger_display,
        world.scheduler.turn,
        t("wield", locale),
        world
            .wielded_item
            .and_then(|id| world.arena.items.get(id))
            .map(|i| t_item(&i.name, locale))
            .unwrap_or_else(|| none_str.to_string()),
        aff_str,
        pack_suffix
    );
    queue!(
        stdout,
        MoveTo(ox, oy + 22),
        SetForegroundColor(Color::Green),
        Print(format!("{status:<80}")),
        ResetColor
    )?;

    // Line 23: Command Bar
    let cmd_help = t("tui.cmd_bar", locale);
    queue!(
        stdout,
        MoveTo(ox, oy + 23),
        SetForegroundColor(Color::DarkGrey),
        Print(format!("{cmd_help:<80}")),
        ResetColor
    )?;

    stdout.flush()
}

/// Message padded (or cut) to exactly 80 columns; the right-aligned seed tag
/// is appended only when it fits without shortening the message (char counts,
/// not bytes). Never wider than 80, so no text is left past the banner.
fn banner_line(message: &str, seed_tag: &str) -> String {
    let (m, t) = (message.chars().count(), seed_tag.chars().count());
    if m + t < 80 {
        format!("{message:<w$}{seed_tag}", w = 80 - t)
    } else {
        let cut: String = message.chars().take(80).collect();
        format!("{cut:<80}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiArgs {
    pub locale: Locale,
    pub seed: u64,
    pub pack_path: Option<std::path::PathBuf>,
}

pub fn parse_args(args: &[String]) -> Result<TuiArgs, String> {
    let mut locale = Locale::En;
    let mut seed = None;
    let mut pack_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lang" | "-l" => {
                if let Some(val) = args.get(i + 1) {
                    locale = Locale::parse(val);
                    i += 1;
                } else {
                    return Err("Missing language code after --lang".into());
                }
            }
            "--uk" => locale = Locale::Uk,
            "--en" => locale = Locale::En,
            "--seed" | "-s" => {
                if let Some(val) = args.get(i + 1) {
                    seed = Some(
                        val.parse::<u64>()
                            .map_err(|_| format!("invalid --seed value: {val}"))?,
                    );
                    i += 1;
                } else {
                    return Err("--seed requires a numeric value".into());
                }
            }
            "--pack" => {
                if let Some(val) = args.get(i + 1) {
                    let pb = std::path::PathBuf::from(val);
                    let resolved = if pb.exists() {
                        pb
                    } else {
                        nethacked_pack::default_packs_dir()
                            .and_then(|d| nethacked_pack::resolve_installed(&d, val))
                            .map(|p| p.path)
                            .ok_or_else(|| {
                                format!(
                                    "Rule pack not found: {val} (not a path or installed pack id)"
                                )
                            })?
                    };
                    pack_path = Some(resolved);
                    i += 1;
                } else {
                    return Err("--pack requires a file path or installed pack id".into());
                }
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
        i += 1;
    }

    let resolved_seed = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42)
    });

    Ok(TuiArgs {
        locale,
        seed: resolved_seed,
        pack_path,
    })
}

/// Parse `--seed N` from the argument list.
#[cfg(test)]
fn parse_seed_args(args: &[String]) -> Result<Option<u64>, String> {
    let Some(pos) = args.iter().position(|a| a == "--seed") else {
        return Ok(None);
    };
    let val = args
        .get(pos + 1)
        .ok_or_else(|| "--seed requires a numeric value".to_string())?;
    val.parse::<u64>()
        .map(Some)
        .map_err(|_| format!("invalid --seed value: {val}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn banner_keeps_full_message() {
        assert_eq!(banner_line("hi", "Seed:5").chars().count(), 80);
        assert!(banner_line("hi", "Seed:5").ends_with("Seed:5"));
        let long = "ж".repeat(75);
        assert_eq!(banner_line(&long, "Seed:5"), format!("{long:<80}"));
    }

    #[test]
    fn banner_never_exceeds_80_columns() {
        // A longer message (e.g. the welcome text) must not spill past column
        // 80, or later 80-column banners leave its tail on screen.
        let long = "x".repeat(120);
        assert_eq!(banner_line(&long, "Seed:5").chars().count(), 80);
        let uk = "ж".repeat(95);
        assert_eq!(banner_line(&uk, "Seed:5").chars().count(), 80);
    }

    #[test]
    fn seed_flag() {
        assert_eq!(parse_seed_args(&s(&["nethacked"])), Ok(None));
        assert_eq!(
            parse_seed_args(&s(&["nethacked", "--seed", "99"])),
            Ok(Some(99))
        );
        assert!(parse_seed_args(&s(&["nethacked", "--seed"])).is_err());
        assert!(parse_seed_args(&s(&["nethacked", "--seed", "x"])).is_err());
    }

    #[test]
    fn test_parse_args_pack_missing() {
        let res = parse_args(&s(&["nethacked", "--pack", "missing.nhpack"]));
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("missing.nhpack"));
    }

    #[test]
    fn parse_args_resolves_installed_pack_by_id() {
        let dir = std::env::temp_dir().join("nethacked-tui-packs");
        let _ = std::fs::remove_dir_all(&dir);
        let hard =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/examples/hard-mode");
        let inst = nethacked_pack::install_pack(&hard, &dir, false).unwrap();
        // Only this test sets NETHACKED_PACKS_DIR.
        std::env::set_var("NETHACKED_PACKS_DIR", &dir);
        let res = parse_args(&s(&["nethacked", "--pack", &inst.id]));
        std::env::remove_var("NETHACKED_PACKS_DIR");
        let parsed = res.expect("installed id resolves");
        assert_eq!(parsed.pack_path.as_deref(), Some(inst.path.as_path()));
    }
}
