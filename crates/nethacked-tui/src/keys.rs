//! Pure key handling for the TUI: a key event plus a tiny context in, a
//! `KeyOutcome` out. No terminal or world access, so it is unit-testable.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nethacked_sim::{ActionAst, Coord, Direction};

/// What the main loop should do in response to a key press.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyOutcome {
    /// Step the simulation with this action.
    Act(ActionAst),
    /// Leave immediately (used by the dead-player screen).
    #[allow(dead_code)]
    Quit,
    /// Ask "really quit? [y/n]" on the message line.
    ConfirmQuit,
    /// Open the inventory to pick an item for the given purpose.
    OpenInventory(InventoryPurpose),
    /// Browse the inventory without choosing anything.
    ViewInventory,
    OpenHelp,
    ToggleLanguage,
    /// Prompt for a direction, then fire the quiver.
    PromptFire,
    /// Prompt for a direction, then zap a wand with `ZAP_ENERGY`.
    PromptZap,
    /// Open the first adjacent closed door.
    OpenDoor,
    /// Close the first adjacent open door.
    CloseDoor,
    /// Mount an adjacent steed, or dismount when riding.
    ToggleMount,
    OpenEnhance,
    OpenConducts,
    /// `#` extended-command prefix (`#e` / `#c`).
    ExtendedCommand,
    /// `^P`: show the previous message (cmd.c:164 `doprev_message`, :1811).
    PrevMessage,
    #[allow(dead_code)]
    Redraw,
    Nothing,
}

/// Why an inventory is being opened. Only `Quaff` and `Quiver` are wired to
/// keys today; the rest are reserved for the item-picker keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InventoryPurpose {
    Drop,
    Wield,
    Quaff,
    Read,
    Eat,
    Sacrifice,
    Apply,
    Rub,
    PriceCheck,
    Quiver,
}

pub struct KeyContext {
    pub player: Coord,
    pub last_dir: Direction,
}

/// Single source of truth for the help screen: every key here must be handled
/// by `handle_key` (enforced by a unit test). The second element is an
/// `nethacked-i18n` key resolved with `t()`.
pub const HELP_KEYS: &[(char, &str)] = &[
    ('h', "help.west"),
    ('j', "help.south"),
    ('k', "help.north"),
    ('l', "help.east"),
    ('y', "help.northwest"),
    ('u', "help.northeast"),
    ('b', "help.southwest"),
    ('n', "help.southeast"),
    ('.', "help.wait"),
    ('5', "help.wait"),
    (',', "help.pickup"),
    ('>', "help.descend"),
    ('<', "help.ascend"),
    ('s', "help.search"),
    ('t', "help.untrap"),
    ('^', "help.untrap"),
    ('f', "help.fire"),
    ('Q', "help.quiver"),
    ('R', "help.ride"),
    ('e', "help.eat"),
    ('q', "help.quaff"),
    ('r', "help.read"),
    ('z', "help.zap"),
    ('p', "help.pay"),
    ('P', "help.pray"),
    ('S', "help.sacrifice"),
    ('o', "help.open"),
    ('c', "help.close"),
    ('K', "help.kick"),
    ('d', "help.drop"),
    ('w', "help.wield"),
    ('x', "help.cast"),
    ('E', "help.enhance"),
    ('C', "help.conducts"),
    ('i', "help.inventory"),
    ('L', "help.lang"),
    ('\\', "help.lang"),
    ('?', "help.help"),
    ('#', "help.ext"),
];

/// Number of rows of the help screen's three-column key grid.
pub const HELP_ROWS: usize = 14;

/// Control-key commands listed on the help screen as `^<key> <description>`,
/// after [`HELP_KEYS`]. Every entry must be handled by `handle_key` with the
/// Ctrl modifier (enforced by a unit test).
pub const HELP_CTRL_KEYS: &[(char, &str)] = &[('p', "help.prevmsg")];

/// True only for `y`/`Y` (after Ukrainian layout mapping, so `н` counts).
pub fn confirm_quit_answer(key: KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Char(c) if matches!(map_ukrainian_key(c), 'y' | 'Y')
    )
}

/// True for keys that dismiss any modal or prompt: Esc, or Ctrl plus any key
/// (so Ctrl-C never selects an item named `c`).
pub fn is_cancel(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL)
}

fn mv(d: Direction) -> KeyOutcome {
    KeyOutcome::Act(ActionAst::Move(d))
}

fn opt(a: Option<ActionAst>) -> KeyOutcome {
    a.map_or(KeyOutcome::Nothing, KeyOutcome::Act)
}

/// Map a key press to what the main loop should do.
///
/// `Ctrl-P` is `^P` prevmsg (cmd.c:164 `doprev_message`, bound to `C('p')` at
/// cmd.c:1811); it is tested before the letter table, where plain `p` is pay.
pub fn handle_key(key: KeyEvent, ctx: &KeyContext) -> KeyOutcome {
    let code = match key.code {
        KeyCode::Char(c) => KeyCode::Char(map_ukrainian_key(c)),
        other => other,
    };
    if code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return KeyOutcome::ConfirmQuit;
    }
    if code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return KeyOutcome::PrevMessage;
    }
    match code {
        KeyCode::Esc => KeyOutcome::ConfirmQuit,
        KeyCode::Char('?') => KeyOutcome::OpenHelp,
        KeyCode::Char('L') | KeyCode::Char('\\') => KeyOutcome::ToggleLanguage,
        KeyCode::Char('h') | KeyCode::Left => mv(Direction::West),
        KeyCode::Char('l') | KeyCode::Right => mv(Direction::East),
        KeyCode::Char('k') | KeyCode::Up => mv(Direction::North),
        KeyCode::Char('j') | KeyCode::Down => mv(Direction::South),
        KeyCode::Char('y') => mv(Direction::NorthWest),
        KeyCode::Char('u') => mv(Direction::NorthEast),
        KeyCode::Char('b') => mv(Direction::SouthWest),
        KeyCode::Char('n') => mv(Direction::SouthEast),
        KeyCode::Char('.') | KeyCode::Char('5') => KeyOutcome::Act(ActionAst::Wait),
        KeyCode::Char(',') => KeyOutcome::Act(ActionAst::PickUp),
        KeyCode::Char('s') => KeyOutcome::Act(ActionAst::Search),
        KeyCode::Char('i') => KeyOutcome::ViewInventory,
        KeyCode::Char('#') => KeyOutcome::ExtendedCommand,
        KeyCode::Char('E') => KeyOutcome::OpenEnhance,
        KeyCode::Char('C') => KeyOutcome::OpenConducts,
        KeyCode::Char('f') => KeyOutcome::PromptFire,
        KeyCode::Char('Q') => KeyOutcome::OpenInventory(InventoryPurpose::Quiver),
        KeyCode::Char('q') => KeyOutcome::OpenInventory(InventoryPurpose::Quaff),
        KeyCode::Char('t') | KeyCode::Char('^') => {
            opt(ctx.player.step(ctx.last_dir).map(ActionAst::Untrap))
        }
        KeyCode::Char('R') => KeyOutcome::ToggleMount,
        KeyCode::Char('p') => KeyOutcome::Act(ActionAst::Pay),
        KeyCode::Char('P') => KeyOutcome::Act(ActionAst::Pray),
        KeyCode::Char('S') => KeyOutcome::Act(ActionAst::Sacrifice(0)),
        KeyCode::Char('d') => KeyOutcome::Act(ActionAst::Drop(0)),
        KeyCode::Char('w') => KeyOutcome::Act(ActionAst::Wield(0)),
        KeyCode::Char('e') => KeyOutcome::Act(ActionAst::Eat(0)),
        KeyCode::Char('x') => KeyOutcome::Act(ActionAst::Cast {
            spell_index: 0,
            dir: ctx.last_dir,
        }),
        KeyCode::Char('r') => KeyOutcome::Act(ActionAst::Read(0)),
        KeyCode::Char('>') => KeyOutcome::Act(ActionAst::Descend),
        KeyCode::Char('<') => KeyOutcome::Act(ActionAst::Ascend),
        KeyCode::Char('z') => KeyOutcome::PromptZap,
        KeyCode::Char('o') => KeyOutcome::OpenDoor,
        KeyCode::Char('c') => KeyOutcome::CloseDoor,
        KeyCode::Char('K') => opt(ctx.player.step(ctx.last_dir).map(ActionAst::Kick)),
        _ => KeyOutcome::Nothing,
    }
}

/// Maps Ukrainian keyboard layout keys to their QWERTY hardware equivalents.
pub fn map_ukrainian_key(c: char) -> char {
    match c {
        'й' => 'q',
        'Й' => 'Q',
        'ц' => 'w',
        'Ц' => 'W',
        'у' => 'e',
        'У' => 'E',
        'к' => 'r',
        'К' => 'R',
        'е' => 't',
        'Е' => 'T',
        'н' => 'y',
        'Н' => 'Y',
        'г' => 'u',
        'Г' => 'U',
        'ш' => 'i',
        'Ш' => 'I',
        'щ' => 'o',
        'Щ' => 'O',
        'з' => 'p',
        'З' => 'P',
        'х' => '[',
        'Х' => '{',
        'ї' => ']',
        'Ї' => '}',
        'ф' => 'a',
        'Ф' => 'A',
        'і' => 's',
        'І' => 'S',
        'в' => 'd',
        'В' => 'D',
        'а' => 'f',
        'А' => 'F',
        'п' => 'g',
        'П' => 'G',
        'р' => 'h',
        'Р' => 'H',
        'о' => 'j',
        'О' => 'J',
        'л' => 'k',
        'Л' => 'K',
        'д' => 'l',
        'Д' => 'L',
        'ж' => ';',
        'Ж' => ':',
        'є' => '\'',
        'Є' => '"',
        'я' => 'z',
        'Я' => 'Z',
        'ч' => 'x',
        'Ч' => 'X',
        'с' => 'c',
        'С' => 'C',
        'м' => 'v',
        'М' => 'V',
        'и' => 'b',
        'И' => 'B',
        'т' => 'n',
        'Т' => 'N',
        'ь' => 'm',
        'Ь' => 'M',
        'б' => ',',
        'Б' => '<',
        'ю' => '.',
        'Ю' => '>',
        'ґ' => '\\',
        'Ґ' => '|',
        '№' => '#',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn ctx() -> KeyContext {
        KeyContext {
            player: Coord::new(10, 10).unwrap(),
            last_dir: Direction::East,
        }
    }

    #[test]
    fn cancel_is_esc_or_ctrl() {
        assert!(is_cancel(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_cancel(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(is_cancel(&KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_cancel(&k('c')));
        assert!(!is_cancel(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::SHIFT
        )));
    }

    #[test]
    fn q_quaffs_not_quits() {
        assert!(matches!(
            handle_key(k('q'), &ctx()),
            KeyOutcome::OpenInventory(InventoryPurpose::Quaff)
        ));
    }
    #[test]
    fn esc_and_ctrl_c_ask_to_quit() {
        assert!(matches!(
            handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &ctx()),
            KeyOutcome::ConfirmQuit
        ));
        assert!(matches!(
            handle_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &ctx()
            ),
            KeyOutcome::ConfirmQuit
        ));
        assert!(
            !matches!(handle_key(k('c'), &ctx()), KeyOutcome::ConfirmQuit),
            "plain c is close-door"
        );
    }
    #[test]
    fn quit_confirmation_accepts_y_and_ukrainian_n_key() {
        assert!(confirm_quit_answer(k('y')));
        assert!(confirm_quit_answer(k('Y')));
        assert!(
            confirm_quit_answer(k('н')),
            "UA layout key on the y position"
        );
        assert!(!confirm_quit_answer(k('n')));
        assert!(!confirm_quit_answer(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE
        )));
    }
    #[test]
    fn help_keys_are_registered_i18n_keys() {
        for (c, key) in HELP_KEYS {
            assert!(
                nethacked_i18n::ALL_KEYS.contains(key),
                "help key '{c}' uses unregistered i18n key {key}"
            );
        }
    }
    #[test]
    fn every_help_key_is_handled() {
        for (c, _) in HELP_KEYS {
            assert!(
                !matches!(handle_key(k(*c), &ctx()), KeyOutcome::Nothing),
                "help lists '{c}' but it does nothing"
            );
        }
        for c in ['d', 'w', 'x', 'E', 'C', '\\', '?'] {
            assert!(
                HELP_KEYS.iter().any(|(h, _)| *h == c),
                "handled key '{c}' missing from help"
            );
        }
    }
    #[test]
    fn cast_uses_last_direction() {
        let c = KeyContext {
            last_dir: Direction::NorthWest,
            ..ctx()
        };
        assert!(matches!(
            handle_key(k('x'), &c),
            KeyOutcome::Act(ActionAst::Cast {
                dir: Direction::NorthWest,
                ..
            })
        ));
    }
    #[test]
    fn ukrainian_ghe_maps_to_backslash() {
        assert_eq!(map_ukrainian_key('ґ'), '\\');
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    #[test]
    fn ctrl_p_is_prevmsg_not_pay() {
        assert_eq!(handle_key(ctrl('p'), &ctx()), KeyOutcome::PrevMessage);
        // The unmodified letters keep their meaning.
        assert_eq!(handle_key(k('p'), &ctx()), KeyOutcome::Act(ActionAst::Pay));
        assert_eq!(handle_key(k('P'), &ctx()), KeyOutcome::Act(ActionAst::Pray));
    }
    #[test]
    fn ctrl_ukrainian_ze_is_prevmsg() {
        // 'з' sits on the QWERTY 'p' key.
        assert_eq!(handle_key(ctrl('з'), &ctx()), KeyOutcome::PrevMessage);
        assert_eq!(handle_key(k('з'), &ctx()), KeyOutcome::Act(ActionAst::Pay));
    }
    #[test]
    fn ctrl_c_still_confirm_quit() {
        assert_eq!(handle_key(ctrl('c'), &ctx()), KeyOutcome::ConfirmQuit);
        assert_eq!(handle_key(ctrl('с'), &ctx()), KeyOutcome::ConfirmQuit);
    }
    #[test]
    fn help_ctrl_keys_are_registered_and_handled() {
        for (c, key) in HELP_CTRL_KEYS {
            assert!(
                nethacked_i18n::ALL_KEYS.contains(key),
                "ctrl help key '{c}' uses unregistered i18n key {key}"
            );
            assert!(
                !matches!(handle_key(ctrl(*c), &ctx()), KeyOutcome::Nothing),
                "help lists ^{c} but it does nothing"
            );
        }
        assert!(HELP_CTRL_KEYS.iter().any(|(c, _)| *c == 'p'));
    }
    #[test]
    fn help_grid_has_room() {
        assert!(
            HELP_KEYS.len() + HELP_CTRL_KEYS.len() <= HELP_ROWS * 3,
            "{} keys do not fit {} rows x 3 columns",
            HELP_KEYS.len() + HELP_CTRL_KEYS.len(),
            HELP_ROWS
        );
    }
}
