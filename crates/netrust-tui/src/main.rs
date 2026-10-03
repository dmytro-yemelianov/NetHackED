//! NetRust Terminal User Interface (TUI).
//!
//! Classic ASCII 80x21 NetHack presentation layer running directly on Crossterm.

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use netrust_dungeon::compute_fov;
use netrust_i18n::{t, t_align, t_hunger_str, Locale};
use netrust_sim::{
    ActionAst, Alignment, CharacterConfig, Coord, Direction, DoorState, GameEvent, Gender,
    HungerState, RaceId, RoleId, SimulationWorld, Tile, COLNO, ROWNO,
};
use std::collections::HashSet;
use std::io::{self, stdout, Stdout};

struct TerminalGuard;

impl TerminalGuard {
    pub fn new(stdout: &mut Stdout) -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, Hide)?;
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

fn select_character(stdout: &mut Stdout, locale: Locale) -> io::Result<Option<CharacterConfig>> {
    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
    let header_desc = if locale == Locale::Uk {
        "  Хто ви? Оберіть початковий клас персонажа:\n\n"
    } else {
        "  Who are you? Pick your starting character role:\n\n"
    };

    let roles_desc = if locale == Locale::Uk {
        concat!(
            "    [v] Валькірія     - Нейтральна Людина (Високе HP, Довгий Меч, Щит)\n",
            "    [w] Маг           - Нейтральна Людина (Магія, Жезл Удару, Сувої)\n",
            "    [b] Варвар        - Хаотичний Орк     (Високе HP, Нищівний Ближній Бій)\n",
            "    [r] Розбійник     - Хаотична Людина   (Кинджал, Короткий Меч, Мішок)\n",
            "    [k] Лицар         - Законний Дворф    (Важка Броня, Довгий Меч)\n",
            "    [m] Монах         - Нейтральна Людина (Бойові Мистецтва, Зцілення)\n",
            "    [h] Цілитель      - Нейтральний Гном  (Зілля Зцілення, Живучість)\n",
            "    [t] Турист        - Нейтральна Людина (Золото, Бездонна Торба)\n",
            "    [a] Археолог      - Законна Людина    (Мішок, Меч, Стародавні Знання)\n\n",
        )
    } else {
        concat!(
            "    [v] Valkyrie      - Neutral Human (High HP, Long Sword, Shield)\n",
            "    [w] Wizard        - Neutral Human (Magic, Wand of Striking, Scrolls)\n",
            "    [b] Barbarian     - Chaotic Orc   (High HP, Brutal Melee)\n",
            "    [r] Rogue         - Chaotic Human (Dagger, Short Sword, Sack)\n",
            "    [k] Knight        - Lawful Dwarf  (Heavy Armor, Long Sword)\n",
            "    [m] Monk          - Neutral Human (Martial Arts, Healing, Teleport)\n",
            "    [h] Healer        - Neutral Gnome (Healing Potions, High Vitality)\n",
            "    [t] Tourist       - Neutral Human (Gold, Bag of Holding, Extra Potions)\n",
            "    [a] Archaeologist - Lawful Human  (Sack, Short Sword, Ancient Lore)\n\n",
        )
    };

    let prompt = if locale == Locale::Uk {
        "  Оберіть роль [v/w/b/r/k/m/h/t/a] або [Enter] для Валькірії: "
    } else {
        "  Press role key [v/w/b/r/k/m/h/t/a] or [Enter] for default Valkyrie: "
    };

    execute!(
        stdout,
        MoveTo(0, 1),
        SetForegroundColor(Color::Cyan),
        Print("================================================================================"),
        MoveTo(0, 2),
        SetForegroundColor(Color::Yellow),
        Print("                 NETRUST: Classic NetHack 5.0 in Rust & Lean 4                 "),
        MoveTo(0, 3),
        SetForegroundColor(Color::Cyan),
        Print("================================================================================"),
        MoveTo(0, 5),
        SetForegroundColor(Color::White),
        Print(header_desc),
        Print(roles_desc),
        SetForegroundColor(Color::Green),
        Print(prompt),
        ResetColor
    )?;

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                KeyCode::Char('v') | KeyCode::Enter | KeyCode::Char(' ') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Валькірія".into() } else { "Valkyrie".into() },
                        role: RoleId::Valkyrie,
                        race: RaceId::Human,
                        gender: Gender::Female,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('w') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Маг".into() } else { "Wizard".into() },
                        role: RoleId::Wizard,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('b') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Варвар".into() } else { "Barbarian".into() },
                        role: RoleId::Barbarian,
                        race: RaceId::Orc,
                        gender: Gender::Male,
                        alignment: Alignment::Chaotic,
                    }));
                }
                KeyCode::Char('r') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Розбійник".into() } else { "Rogue".into() },
                        role: RoleId::Rogue,
                        race: RaceId::Human,
                        gender: Gender::Female,
                        alignment: Alignment::Chaotic,
                    }));
                }
                KeyCode::Char('k') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Лицар".into() } else { "Knight".into() },
                        role: RoleId::Knight,
                        race: RaceId::Dwarf,
                        gender: Gender::Male,
                        alignment: Alignment::Lawful,
                    }));
                }
                KeyCode::Char('m') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Монах".into() } else { "Monk".into() },
                        role: RoleId::Monk,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('h') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Цілитель".into() } else { "Healer".into() },
                        role: RoleId::Healer,
                        race: RaceId::Gnome,
                        gender: Gender::Female,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('t') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Турист".into() } else { "Tourist".into() },
                        role: RoleId::Tourist,
                        race: RaceId::Human,
                        gender: Gender::Male,
                        alignment: Alignment::Neutral,
                    }));
                }
                KeyCode::Char('a') => {
                    return Ok(Some(CharacterConfig {
                        name: if locale == Locale::Uk { "Археолог".into() } else { "Archaeologist".into() },
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

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut locale = Locale::En;
    for (i, arg) in args.iter().enumerate() {
        if arg == "--lang" || arg == "-l" {
            if let Some(val) = args.get(i + 1) {
                locale = Locale::parse(val);
            }
        } else if arg == "--uk" {
            locale = Locale::Uk;
        } else if arg == "--en" {
            locale = Locale::En;
        }
    }

    let mut stdout = stdout();
    let _guard = TerminalGuard::new(&mut stdout)?;

    let config = match select_character(&mut stdout, locale)? {
        Some(cfg) => cfg,
        None => return Ok(()),
    };

    let char_name = config.name.clone();
    let mut world = SimulationWorld::new_with_character(42, config);
    world.set_locale(locale);
    let mut message = if locale == Locale::Uk {
        format!("Ласкаво просимо до NetRust, {}! Класичний NetHack 5.0, формалізований у Lean 4.", char_name)
    } else {
        format!("Welcome to NetRust, {}! Classic NetHack 5.0 formalized & verified in Lean 4.", char_name)
    };
    let mut last_dir = Direction::East;

    loop {
        render(&mut stdout, &world, &message)?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let player_opt = world.arena.actors.get(world.player_id).cloned();
            let Some(player) = player_opt else { break; };
            if player.is_dead {
                message = if world.locale == Locale::Uk {
                    "Ви загинули... Натисніть 'q' для виходу.".into()
                } else {
                    "You have died... Press 'q' to quit.".into()
                };
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
                continue;
            }

            let action = match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('L') | KeyCode::Char('\\') => {
                    let new_loc = if world.locale == Locale::Uk { Locale::En } else { Locale::Uk };
                    world.set_locale(new_loc);
                    message = if new_loc == Locale::Uk {
                        "Мову інтерфейсу перемкнено на українську (uk-UA).".into()
                    } else {
                        "Interface language switched to English (en-US).".into()
                    };
                    None
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    last_dir = Direction::West;
                    Some(ActionAst::Move(Direction::West))
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    last_dir = Direction::East;
                    Some(ActionAst::Move(Direction::East))
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    last_dir = Direction::North;
                    Some(ActionAst::Move(Direction::North))
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    last_dir = Direction::South;
                    Some(ActionAst::Move(Direction::South))
                }
                KeyCode::Char('y') => {
                    last_dir = Direction::NorthWest;
                    Some(ActionAst::Move(Direction::NorthWest))
                }
                KeyCode::Char('u') => {
                    last_dir = Direction::NorthEast;
                    Some(ActionAst::Move(Direction::NorthEast))
                }
                KeyCode::Char('b') => {
                    last_dir = Direction::SouthWest;
                    Some(ActionAst::Move(Direction::SouthWest))
                }
                KeyCode::Char('n') => {
                    last_dir = Direction::SouthEast;
                    Some(ActionAst::Move(Direction::SouthEast))
                }
                KeyCode::Char('.') | KeyCode::Char('5') => Some(ActionAst::Wait),
                KeyCode::Char(',') => Some(ActionAst::PickUp),
                KeyCode::Char('p') => Some(ActionAst::Pay),
                KeyCode::Char('P') => Some(ActionAst::Pray),
                KeyCode::Char('S') => Some(ActionAst::Sacrifice(0)),
                KeyCode::Char('d') => Some(ActionAst::Drop(0)),
                KeyCode::Char('w') => Some(ActionAst::Wield(0)),
                KeyCode::Char('e') => Some(ActionAst::Eat(0)),
                KeyCode::Char('x') => Some(ActionAst::Cast { spell_index: 0, dir: last_dir }),
                KeyCode::Char('r') => Some(ActionAst::Read(0)),
                KeyCode::Char('>') => Some(ActionAst::Descend),
                KeyCode::Char('<') => Some(ActionAst::Ascend),
                KeyCode::Char('z') => {
                    Some(ActionAst::ZapWand { dir: last_dir, energy: 6 })
                }
                KeyCode::Char('o') => {
                    // Open door in adjacent direction
                    let adj_door = Direction::all_compass().iter().find_map(|&d| {
                        player.coord.step(d).and_then(|c| {
                            if matches!(world.level.get_tile(c), Tile::Door { state: DoorState::Closed, .. }) {
                                Some(c)
                            } else {
                                None
                            }
                        })
                    });
                    adj_door.map(ActionAst::OpenDoor)
                }
                KeyCode::Char('c') => {
                    // Close door in adjacent direction
                    let adj_door = Direction::all_compass().iter().find_map(|&d| {
                        player.coord.step(d).and_then(|c| {
                            if matches!(world.level.get_tile(c), Tile::Door { state: DoorState::Open, .. }) {
                                Some(c)
                            } else {
                                None
                            }
                        })
                    });
                    adj_door.map(ActionAst::CloseDoor)
                }
                KeyCode::Char('K') => {
                    // Kick in last moved direction
                    player.coord.step(last_dir).map(ActionAst::Kick)
                }
                _ => None,
            };

            if let Some(act) = action {
                let events = world.step_player_action(act);
                let loc = world.locale;
                if let Some(last_msg) = events.iter().filter_map(|e| match e {
                    GameEvent::LogMessage { text } => Some(text.clone()),
                    GameEvent::AttackLanded { damage, lethal, .. } => {
                        let hit_str = if loc == Locale::Uk {
                            format!("Ви влучаєте у чудовисько на {} шкоди!{}", damage, if *lethal { " Воно гине!" } else { "" })
                        } else {
                            format!("You hit the monster for {} damage!{}", damage, if *lethal { " It dies!" } else { "" })
                        };
                        Some(hit_str)
                    }
                    GameEvent::AttackMissed { .. } => Some(if loc == Locale::Uk { "Ви промахуєтесь повз чудовисько.".into() } else { "You miss the monster.".into() }),
                    GameEvent::DoorToggled { new_state, .. } => Some(if loc == Locale::Uk { format!("Стан дверей: {:?}.", new_state) } else { format!("The door is now {:?}.", new_state) }),
                    GameEvent::LevelChanged { to_depth, .. } => Some(if loc == Locale::Uk { format!("Ви переходите на рівень {}.", to_depth) } else { format!("You enter dungeon level {}.", to_depth) }),
                    _ => None,
                }).last() {
                    message = last_msg;
                }
            }
        }
    }

    Ok(())
}

fn render(stdout: &mut Stdout, world: &SimulationWorld, message: &str) -> io::Result<()> {
    let p_coord = world
        .arena
        .actors
        .get(world.player_id)
        .map(|p| p.coord)
        .unwrap_or(Coord::new_unchecked(0, 0));

    let visible: HashSet<Coord> = compute_fov(&world.level, p_coord, 8);

    // Line 0: Message banner
    execute!(stdout, MoveTo(0, 0), SetForegroundColor(Color::Yellow), Print(format!("{:<80}", message)), ResetColor)?;

    // Lines 1..=21: 80x21 Dungeon grid
    for y in 0..ROWNO {
        execute!(stdout, MoveTo(0, (y + 1) as u16))?;
        for x in 0..COLNO {
            let c = Coord::new_unchecked(x, y);
            if !visible.contains(&c) {
                execute!(stdout, Print(" "))?;
                continue;
            }

            // Actor priority
            if let Some(actor_id) = world.actor_at(c) {
                if actor_id == world.player_id {
                    execute!(stdout, SetForegroundColor(Color::White), Print("@"), ResetColor)?;
                    continue;
                } else if let Some(actor) = world.arena.actors.get(actor_id) {
                    let ch = actor.name.chars().next().unwrap_or('m').to_ascii_lowercase();
                    execute!(stdout, SetForegroundColor(Color::Red), Print(ch), ResetColor)?;
                    continue;
                }
            }

            // Floor items priority
            let floor_items = world.arena.items_at_floor(c);
            if let Some(&item_id) = floor_items.first() {
                if let Some(item) = world.arena.items.get(item_id) {
                    let sym = item.class.symbol();
                    execute!(stdout, SetForegroundColor(Color::Cyan), Print(sym), ResetColor)?;
                    continue;
                }
            }

            // Tile rendering
            match world.level.get_tile(c) {
                Tile::Stone => execute!(stdout, Print(" "))?,
                Tile::Wall { horizontal } => {
                    execute!(stdout, SetForegroundColor(Color::DarkGrey), Print(if *horizontal { '-' } else { '|' }), ResetColor)?;
                }
                Tile::Room => {
                    execute!(stdout, SetForegroundColor(Color::Grey), Print("."), ResetColor)?;
                }
                Tile::Corr => {
                    execute!(stdout, SetForegroundColor(Color::DarkGrey), Print("#"), ResetColor)?;
                }
                Tile::Door { state, .. } => {
                    let sym = match state {
                        DoorState::Open => '/',
                        DoorState::Closed | DoorState::Locked => '+',
                        DoorState::Broken => '*',
                    };
                    execute!(stdout, SetForegroundColor(Color::Yellow), Print(sym), ResetColor)?;
                }
                Tile::SecretDoor { .. } => execute!(stdout, Print(" "))?,
                Tile::Stairs { up } => {
                    execute!(stdout, SetForegroundColor(Color::Magenta), Print(if *up { '<' } else { '>' }), ResetColor)?;
                }
                Tile::BranchStairs { up, .. } => {
                    execute!(stdout, SetForegroundColor(Color::Cyan), Print(if *up { '<' } else { '>' }), ResetColor)?;
                }
                Tile::Pit { filled } => {
                    let (sym, col) = if *filled { ('.', Color::Grey) } else { ('0', Color::DarkYellow) };
                    execute!(stdout, SetForegroundColor(col), Print(sym), ResetColor)?;
                }
                Tile::Altar { .. } => {
                    execute!(stdout, SetForegroundColor(Color::White), Print("_"), ResetColor)?;
                }
                Tile::HighAltar { .. } => {
                    execute!(stdout, SetForegroundColor(Color::Yellow), Print("_"), ResetColor)?;
                }
                Tile::Drawbridge { open } => {
                    if *open {
                        execute!(stdout, SetForegroundColor(Color::DarkGrey), Print("."), ResetColor)?;
                    } else {
                        execute!(stdout, SetForegroundColor(Color::DarkYellow), Print("#"), ResetColor)?;
                    }
                }
                Tile::Moat => {
                    execute!(stdout, SetForegroundColor(Color::Cyan), Print("}"), ResetColor)?;
                }
                Tile::Pool { frozen } => {
                    execute!(stdout, SetForegroundColor(Color::Blue), Print(if *frozen { '=' } else { '}' }), ResetColor)?;
                }
                Tile::Lava => {
                    execute!(stdout, SetForegroundColor(Color::Red), Print("^"), ResetColor)?;
                }
            }
        }
    }

    // Line 22: Botl Status Line
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
    let hunger_display = if hunger_code.is_empty() { "" } else { t_hunger_str(hunger_code, locale) };
    let none_str = if locale == Locale::Uk { "пусто" } else { "none" };

    let status = format!(
        "{}:{} {}:{:<2} {}:{} {}:{}({}) {}:{}({}) {}:{:<2} {:<8} {}:{:<4} {}:{}",
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
        t("turn", locale),
        world.scheduler.turn,
        t("wield", locale),
        world.wielded_item.and_then(|id| world.arena.items.get(id)).map(|i| i.name.as_str()).unwrap_or(none_str)
    );
    execute!(stdout, MoveTo(0, 22), SetForegroundColor(Color::Green), Print(format!("{:<80}", status)), ResetColor)?;

    // Line 23: Command Bar
    let cmd_help = if locale == Locale::Uk {
        "[h/j/k/l: Рух | e: Їсти | x: Закляття | r: Читати | z: Жезл | ,: Взяти | P: Молитва | S: Жертва | L: Мова | q: Вихід]"
    } else {
        "[h/j/k/l: Move | e: Eat | x: Cast | r: Read | z: Zap | ,: PickUp | P: Pray | S: Sac | L: Lang | q: Quit]"
    };
    execute!(stdout, MoveTo(0, 23), SetForegroundColor(Color::DarkGrey), Print(format!("{:<80}", cmd_help)), ResetColor)?;

    Ok(())
}
