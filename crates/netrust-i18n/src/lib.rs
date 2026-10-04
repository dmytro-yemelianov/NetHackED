//! Complete Internationalization (i18n) Engine for NetRust.
//!
//! Provides first-class English (canonical NetHack) and Ukrainian (uk-UA)
//! localized ontologies, UI strings, message formatters, and status descriptions.

pub use netrust_types::Locale;
use netrust_types::{Alignment, BranchId, Buc};

/// Every key handled by [`t`].
pub const ALL_KEYS: &[&str] = &[
    "hero",
    "align",
    "dlvl",
    "gold",
    "hp",
    "pw",
    "ac",
    "nutr",
    "turn",
    "eat",
    "cast",
    "read",
    "pickup",
    "pay",
    "pray",
    "sacrifice",
    "wield",
    "drop",
    "descend",
    "ascend",
    "wait",
    "leaderboard",
    "character",
    "reset",
    "inventory",
    "quaff",
    "dip",
    "rub",
    "price_check",
    "engrave",
    "starved",
    "fainted_death",
    "role.valkyrie",
    "role.valkyrie.race",
    "role.valkyrie.desc",
    "role.wizard",
    "role.wizard.race",
    "role.wizard.desc",
    "role.barbarian",
    "role.barbarian.race",
    "role.barbarian.desc",
    "role.rogue",
    "role.rogue.race",
    "role.rogue.desc",
    "role.knight",
    "role.knight.race",
    "role.knight.desc",
    "role.monk",
    "role.monk.race",
    "role.monk.desc",
    "role.healer",
    "role.healer.race",
    "role.healer.desc",
    "role.tourist",
    "role.tourist.race",
    "role.tourist.desc",
    "role.archaeologist",
    "role.archaeologist.race",
    "role.archaeologist.desc",
    "tui.pick_role",
    "tui.role_prompt",
    "tui.inventory_title",
    "tui.pack_empty",
    "tui.in_hand",
    "tui.weight",
    "tui.conducts_title",
    "tui.conduct_active",
    "tui.conduct_broken",
    "conduct.pacifist",
    "conduct.vegan",
    "conduct.vegetarian",
    "conduct.atheist",
    "conduct.illiterate",
    "conduct.genocideless",
    "conduct.polypileless",
    "conduct.wishless",
    "tui.return_prompt",
    "tui.enhance_prompt",
    "tui.help_title",
    "tui.help_quit",
    "tui.seed",
    "tui.died",
    "tui.quit_prompt",
    "tui.lang_switched",
    "tui.ext_prompt",
    "tui.fire_prompt",
    "tui.zap_prompt",
    "tui.miss",
    "tui.none",
    "tui.cmd_bar",
    "help.west",
    "help.south",
    "help.north",
    "help.east",
    "help.northwest",
    "help.northeast",
    "help.southwest",
    "help.southeast",
    "help.wait",
    "help.pickup",
    "help.descend",
    "help.ascend",
    "help.search",
    "help.untrap",
    "help.fire",
    "help.quiver",
    "help.ride",
    "help.eat",
    "help.quaff",
    "help.read",
    "help.zap",
    "help.pay",
    "help.pray",
    "help.sacrifice",
    "help.open",
    "help.close",
    "help.kick",
    "help.drop",
    "help.wield",
    "help.cast",
    "help.enhance",
    "help.conducts",
    "help.inventory",
    "help.lang",
    "help.help",
    "help.ext",
];

/// Lowercase monster/item names that are genuinely identical in Ukrainian.
pub const UK_IDENTICAL_NAMES: &[&str] = &[];

/// `t()` keys whose Ukrainian text may equal the English one.
pub const UK_IDENTICAL_KEYS: &[&str] = &[];

/// Translates general UI keywords and action names.
pub fn t(key: &'static str, locale: Locale) -> &'static str {
    match locale {
        Locale::En => match key {
            "hero" => "Hero",
            "align" => "Align",
            "dlvl" => "Dlvl",
            "gold" => "Gold",
            "hp" => "HP",
            "pw" => "Pw",
            "ac" => "AC",
            "nutr" => "Nutr",
            "turn" => "Turn",
            "eat" => "Eat",
            "cast" => "Cast",
            "read" => "Read",
            "pickup" => "PickUp",
            "pay" => "Pay",
            "pray" => "Pray",
            "sacrifice" => "Sacrifice",
            "wield" => "Wield",
            "drop" => "Drop",
            "descend" => "Descend",
            "ascend" => "Ascend",
            "wait" => "Wait",
            "leaderboard" => "Leaderboard & AI Arena",
            "character" => "Character",
            "reset" => "Reset",
            "inventory" => "Inventory",
            "quaff" => "Quaff",
            "dip" => "Dip",
            "rub" => "Rub",
            "price_check" => "Appraise",
            "engrave" => "Engrave",
            "starved" => "You die from starvation.",
            "fainted_death" => "You faint from lack of food and die.",
            "role.valkyrie" => "Valkyrie",
            "role.valkyrie.race" => "Neutral Human",
            "role.valkyrie.desc" => "(High HP, Long Sword, Shield)",
            "role.wizard" => "Wizard",
            "role.wizard.race" => "Neutral Human",
            "role.wizard.desc" => "(Magic, Wand of Striking, Scrolls)",
            "role.barbarian" => "Barbarian",
            "role.barbarian.race" => "Chaotic Orc",
            "role.barbarian.desc" => "(High HP, Brutal Melee)",
            "role.rogue" => "Rogue",
            "role.rogue.race" => "Chaotic Human",
            "role.rogue.desc" => "(Dagger, Short Sword, Sack)",
            "role.knight" => "Knight",
            "role.knight.race" => "Lawful Dwarf",
            "role.knight.desc" => "(Heavy Armor, Long Sword)",
            "role.monk" => "Monk",
            "role.monk.race" => "Neutral Human",
            "role.monk.desc" => "(Martial Arts, Healing, Teleport)",
            "role.healer" => "Healer",
            "role.healer.race" => "Neutral Gnome",
            "role.healer.desc" => "(Healing Potions, High Vitality)",
            "role.tourist" => "Tourist",
            "role.tourist.race" => "Neutral Human",
            "role.tourist.desc" => "(Gold, Bag of Holding, Extra Potions)",
            "role.archaeologist" => "Archaeologist",
            "role.archaeologist.race" => "Lawful Human",
            "role.archaeologist.desc" => "(Sack, Short Sword, Ancient Lore)",
            "tui.pick_role" => "Who are you? Pick your starting character role:",
            "tui.role_prompt" => {
                "Press role key [v/w/b/r/k/m/h/t/a] or [Enter] for default Valkyrie: "
            }
            "tui.inventory_title" => {
                "=== CHARACTER INVENTORY (Press item letter or Esc to close) ==="
            }
            "tui.pack_empty" => "Your pack is empty.",
            "tui.in_hand" => " (weapon in hand)",
            "tui.weight" => "weight",
            "tui.conducts_title" => "=== VOLUNTARY CONDUCTS TRACKER (NetHack Formal Conducts) ===",
            "tui.conduct_active" => "[ACTIVE]",
            "tui.conduct_broken" => "[BROKEN]",
            "conduct.pacifist" => "Pacifist (Never kill any creature directly)",
            "conduct.vegan" => "Vegan (Never consume animal products)",
            "conduct.vegetarian" => "Vegetarian (Never consume meat)",
            "conduct.atheist" => "Atheist (Never pray or sacrifice at altars)",
            "conduct.illiterate" => "Illiterate (Never read scrolls or books)",
            "conduct.genocideless" => "Genocideless (Never cast or read genocide)",
            "conduct.polypileless" => "Polypileless (Never polypile items)",
            "conduct.wishless" => "Wishless (Never wish for items)",
            "tui.return_prompt" => "Press Esc or Space to return to the dungeon...",
            "tui.enhance_prompt" => "Press skill letter [a-g] to enhance skill, or Esc to exit.",
            "tui.help_title" => "=== NETRUST COMMAND & KEYBINDING REFERENCE ===",
            "tui.help_quit" => "Esc / Ctrl-C : quit (asks y/n)",
            "tui.seed" => "Seed",
            "tui.died" => "You have died... Press Esc to quit.",
            "tui.quit_prompt" => "Really quit? [y/n]",
            "tui.lang_switched" => "Interface language switched to English (en-US).",
            "tui.ext_prompt" => "#command: [e]nhance (skills) | [c]onduct (challenges): ",
            "tui.fire_prompt" => "In what direction? [h/j/k/l/y/u/b/n]: ",
            "tui.zap_prompt" => "Zap wand in what direction? [h/j/k/l/y/u/b/n]: ",
            "tui.miss" => "You miss the monster.",
            "tui.none" => "none",
            "tui.cmd_bar" => {
                "[hjkl: Move | s: Search | f: Fire | #: Commands | ?: Help | Esc: Quit]"
            }
            "help.west" => "Move west",
            "help.south" => "Move south",
            "help.north" => "Move north",
            "help.east" => "Move east",
            "help.northwest" => "Move northwest",
            "help.northeast" => "Move northeast",
            "help.southwest" => "Move southwest",
            "help.southeast" => "Move southeast",
            "help.wait" => "Wait a turn",
            "help.pickup" => "Pick up",
            "help.descend" => "Descend stairs",
            "help.ascend" => "Ascend stairs",
            "help.search" => "Search",
            "help.untrap" => "Untrap (facing)",
            "help.fire" => "Fire quiver",
            "help.quiver" => "Select quiver",
            "help.ride" => "Ride / dismount",
            "help.eat" => "Eat",
            "help.quaff" => "Quaff potion",
            "help.read" => "Read scroll",
            "help.zap" => "Zap wand",
            "help.pay" => "Pay shopkeeper",
            "help.pray" => "Pray",
            "help.sacrifice" => "Sacrifice",
            "help.open" => "Open door",
            "help.close" => "Close door",
            "help.kick" => "Kick (facing)",
            "help.drop" => "Drop item",
            "help.wield" => "Wield weapon",
            "help.cast" => "Cast spell",
            "help.enhance" => "Enhance skills",
            "help.conducts" => "Conducts",
            "help.inventory" => "Inventory",
            "help.lang" => "Language UK/EN",
            "help.help" => "This help",
            "help.ext" => "Ext. command (#e/#c)",
            _ => key,
        },
        Locale::Uk => match key {
            "hero" => "Герой",
            "align" => "Шлях",
            "dlvl" => "Рівень",
            "gold" => "Золото",
            "hp" => "Здоров'я",
            "pw" => "Мана",
            "ac" => "Броня",
            "nutr" => "Ситість",
            "turn" => "Хід",
            "eat" => "Їсти",
            "cast" => "Закляття",
            "read" => "Читати",
            "pickup" => "Підняти",
            "pay" => "Платити",
            "pray" => "Молитися",
            "sacrifice" => "Жертва",
            "wield" => "Зброя",
            "drop" => "Кинути",
            "descend" => "Вниз",
            "ascend" => "Вгору",
            "wait" => "Чекати",
            "leaderboard" => "Таблиця лідерів & ШІ Арена",
            "character" => "Персонаж",
            "reset" => "Скинути",
            "inventory" => "Інвентар",
            "quaff" => "Пити",
            "dip" => "Занурити",
            "rub" => "Терти",
            "price_check" => "Оцінити",
            "engrave" => "Викарбувати",
            "starved" => "Ви помираєте від голоду.",
            "fainted_death" => "Ви непритомнієте від браку їжі й помираєте.",
            "role.valkyrie" => "Валькірія",
            "role.valkyrie.race" => "Нейтральна Людина",
            "role.valkyrie.desc" => "(Високе HP, Довгий Меч, Щит)",
            "role.wizard" => "Маг",
            "role.wizard.race" => "Нейтральна Людина",
            "role.wizard.desc" => "(Магія, Жезл Удару, Сувої)",
            "role.barbarian" => "Варвар",
            "role.barbarian.race" => "Хаотичний Орк",
            "role.barbarian.desc" => "(Високе HP, Нищівний Ближній Бій)",
            "role.rogue" => "Розбійник",
            "role.rogue.race" => "Хаотична Людина",
            "role.rogue.desc" => "(Кинджал, Короткий Меч, Мішок)",
            "role.knight" => "Лицар",
            "role.knight.race" => "Законний Дворф",
            "role.knight.desc" => "(Важка Броня, Довгий Меч)",
            "role.monk" => "Монах",
            "role.monk.race" => "Нейтральна Людина",
            "role.monk.desc" => "(Бойові Мистецтва, Зцілення)",
            "role.healer" => "Цілитель",
            "role.healer.race" => "Нейтральний Гном",
            "role.healer.desc" => "(Зілля Зцілення, Живучість)",
            "role.tourist" => "Турист",
            "role.tourist.race" => "Нейтральна Людина",
            "role.tourist.desc" => "(Золото, Бездонна Торба)",
            "role.archaeologist" => "Археолог",
            "role.archaeologist.race" => "Законна Людина",
            "role.archaeologist.desc" => "(Мішок, Меч, Стародавні Знання)",
            "tui.pick_role" => "Хто ви? Оберіть початковий клас персонажа:",
            "tui.role_prompt" => "Оберіть роль [v/w/b/r/k/m/h/t/a] або [Enter] для Валькірії: ",
            "tui.inventory_title" => {
                "=== ІНВЕНТАР ПЕРСОНАЖА (Натисніть букву або Esc для закриття) ==="
            }
            "tui.pack_empty" => "Ваш інвентар порожній.",
            "tui.in_hand" => " (в руці)",
            "tui.weight" => "вага",
            "tui.conducts_title" => "=== ДОБРОВІЛЬНІ ОБІТНИЦІ (NetHack Voluntary Conducts) ===",
            "tui.conduct_active" => "[НЕПОРУШЕНО]",
            "tui.conduct_broken" => "[ПОРУШЕНО]",
            "conduct.pacifist" => "Пацифіст (ніколи не вбивати істот безпосередньо)",
            "conduct.vegan" => "Веган (ніколи не вживати продуктів тваринного походження)",
            "conduct.vegetarian" => "Вегетаріанець (ніколи не їсти м'яса)",
            "conduct.atheist" => "Атеїст (ніколи не молитися і не приносити жертв на вівтарях)",
            "conduct.illiterate" => "Неписьменний (ніколи не читати сувоїв чи книг)",
            "conduct.genocideless" => "Без геноциду (ніколи не застосовувати геноцид)",
            "conduct.polypileless" => "Без поліпайлу (ніколи не перетворювати купи предметів)",
            "conduct.wishless" => "Без бажань (ніколи не загадувати бажань)",
            "tui.return_prompt" => "Натисніть Esc або Пробіл для повернення до гри...",
            "tui.enhance_prompt" => {
                "Натисніть букву [a-g] для покращення навички або Esc для закриття."
            }
            "tui.help_title" => "=== NETRUST ДОВІДНИК КЛАВІШ ТА КОМАНД ===",
            "tui.help_quit" => "Esc / Ctrl-C : вихід (запитує y/n)",
            "tui.seed" => "Зерно",
            "tui.died" => "Ви загинули... Натисніть Esc для виходу.",
            "tui.quit_prompt" => "Справді вийти? [y/n]",
            "tui.lang_switched" => "Мову інтерфейсу перемкнено на українську (uk-UA).",
            "tui.ext_prompt" => "#команда: [e]nhance (навички) | [c]onduct (обітниці): ",
            "tui.fire_prompt" => "У якому напрямку вистрілити? [h/j/k/l/y/u/b/n]: ",
            "tui.zap_prompt" => "Куди спрямувати жезл? [h/j/k/l/y/u/b/n]: ",
            "tui.miss" => "Ви промахуєтесь повз чудовисько.",
            "tui.none" => "пусто",
            "tui.cmd_bar" => {
                "[hjkl: Рух | s: Пошук | f: Вогонь | #: Команди | ?: Довідка | Esc: Вихід]"
            }
            "help.west" => "Рух на захід",
            "help.south" => "Рух на південь",
            "help.north" => "Рух на північ",
            "help.east" => "Рух на схід",
            "help.northwest" => "Рух на північний захід",
            "help.northeast" => "Рух на північний схід",
            "help.southwest" => "Рух на південний захід",
            "help.southeast" => "Рух на південний схід",
            "help.wait" => "Зачекати хід",
            "help.pickup" => "Підібрати",
            "help.descend" => "Спуститися",
            "help.ascend" => "Піднятися",
            "help.search" => "Пошук",
            "help.untrap" => "Знешкодити пастку",
            "help.fire" => "Вистрілити",
            "help.quiver" => "Обрати боєприпаси",
            "help.ride" => "Осідлати / зійти",
            "help.eat" => "З'їсти",
            "help.quaff" => "Випити зілля",
            "help.read" => "Прочитати сувій",
            "help.zap" => "Жезл",
            "help.pay" => "Заплатити",
            "help.pray" => "Помолитися",
            "help.sacrifice" => "Пожертвувати",
            "help.open" => "Відкрити двері",
            "help.close" => "Закрити двері",
            "help.kick" => "Вдарити ногою",
            "help.drop" => "Кинути предмет",
            "help.wield" => "Взяти зброю",
            "help.cast" => "Заклинання",
            "help.enhance" => "Покращити навички",
            "help.conducts" => "Обітниці",
            "help.inventory" => "Інвентар",
            "help.lang" => "Мова UK/EN",
            "help.help" => "Ця довідка",
            "help.ext" => "Розш. команда (#e/#c)",
            _ => key,
        },
    }
}

/// Translates Alignment.
pub fn t_align(align: Alignment, locale: Locale) -> &'static str {
    match (align, locale) {
        (Alignment::Lawful, Locale::En) => "Lawful",
        (Alignment::Lawful, Locale::Uk) => "Законний",
        (Alignment::Neutral, Locale::En) => "Neutral",
        (Alignment::Neutral, Locale::Uk) => "Нейтральний",
        (Alignment::Chaotic, Locale::En) => "Chaotic",
        (Alignment::Chaotic, Locale::Uk) => "Хаотичний",
        (Alignment::Unaligned, Locale::En) => "Unaligned",
        (Alignment::Unaligned, Locale::Uk) => "Нейтралітет",
    }
}

/// Translates BUC lattice status.
pub fn t_buc(buc: Buc, locale: Locale) -> &'static str {
    match (buc, locale) {
        (Buc::Blessed, Locale::En) => "blessed",
        (Buc::Blessed, Locale::Uk) => "благословенний",
        (Buc::Uncursed, Locale::En) => "uncursed",
        (Buc::Uncursed, Locale::Uk) => "непроклятий",
        (Buc::Cursed, Locale::En) => "cursed",
        (Buc::Cursed, Locale::Uk) => "проклятий",
    }
}

/// Translates Dungeon Branch.
pub fn t_branch(branch: BranchId, locale: Locale) -> &'static str {
    match (branch, locale) {
        (BranchId::DungeonsOfDoom, Locale::En) => "Dungeons of Doom",
        (BranchId::DungeonsOfDoom, Locale::Uk) => "Підземелля Загибелі",
        (BranchId::GnomishMines, Locale::En) => "Gnomish Mines",
        (BranchId::GnomishMines, Locale::Uk) => "Гном'ячі Копальні",
        (BranchId::Sokoban, Locale::En) => "Sokoban",
        (BranchId::Sokoban, Locale::Uk) => "Сокобан",
        (BranchId::Gehennom, Locale::En) => "Gehennom",
        (BranchId::Gehennom, Locale::Uk) => "Геєнна",
        (BranchId::AstralPlane, Locale::En) => "Astral Plane",
        (BranchId::AstralPlane, Locale::Uk) => "Астральний План",
        (BranchId::Quest, Locale::En) => "The Quest",
        (BranchId::Quest, Locale::Uk) => "Завдання Героя",
        (BranchId::WizardsTower, Locale::En) => "Wizard's Tower",
        (BranchId::WizardsTower, Locale::Uk) => "Вежа Чарівника",
        (BranchId::VladsTower, Locale::En) => "Vlad's Tower",
        (BranchId::VladsTower, Locale::Uk) => "Вежа Влада",
        (BranchId::FortLudios, Locale::En) => "Fort Ludios",
        (BranchId::FortLudios, Locale::Uk) => "Форт Лудіос",
        (BranchId::RogueLevel, Locale::En) => "Rogue Level",
        (BranchId::RogueLevel, Locale::Uk) => "Рівень Rogue",
    }
}

/// Translates Hunger state strings.
pub fn t_hunger_str(state_str: &str, locale: Locale) -> &'static str {
    match locale {
        Locale::En => match state_str.to_lowercase().as_str() {
            "satiated" => "Satiated",
            "normal" => "Normal",
            "hungry" => "Hungry",
            "weak" => "Weak",
            "fainting" => "Fainting",
            "starved" => "Starved",
            _ => "Normal",
        },
        Locale::Uk => match state_str.to_lowercase().as_str() {
            "satiated" => "Пересичений",
            "normal" => "Ситий",
            "hungry" => "Голодний",
            "weak" => "Слабкість",
            "fainting" => "Непритомність",
            "starved" => "Виснажений",
            _ => "Ситий",
        },
    }
}

/// Translates a monster or item name for embedding in a message.
/// Tries monster names first, then item names; unknown names pass through.
/// C `upstart`/`Monnam`: upper-case the first character.
fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn tn(name: &str, locale: Locale) -> String {
    if locale == Locale::En {
        return name.to_string();
    }
    let m = t_monster(name, locale);
    if m != name {
        return m;
    }
    t_item(name, locale)
}

/// Message formatters with full locale support.
pub struct Messages;

impl Messages {
    pub fn welcome(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Welcome to NetRust! Core mechanics modeled in Lean 4.",
            Locale::Uk => "Ласкаво просимо до NetRust! Ключові механіки змодельовано в Lean 4.",
        }
    }

    pub fn tui_welcome(name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Welcome to NetRust, {name}! 100% canonical NetHack 5.0 formalized & verified in Lean 4."),
            Locale::Uk => format!("Ласкаво просимо до NetRust, {name}! 100% канонічний NetHack 5.0, формалізований у Lean 4."),
        }
    }

    pub fn enhance_title(slots: impl std::fmt::Display, locale: Locale) -> String {
        match locale {
            Locale::En => format!(
                "=== WEAPON SKILLS PROFICIENCY TREE (#enhance) | Available Slots: {slots} ==="
            ),
            Locale::Uk => {
                format!("=== ДЕРЕВО НАВИЧОК ЗБРОЇ (#enhance) | Вільних слотів: {slots} ===")
            }
        }
    }

    pub fn inventory_page_footer(page: usize, pages: usize, locale: Locale) -> String {
        match locale {
            Locale::En => format!("(page {page}/{pages}, >/< to turn)"),
            Locale::Uk => format!("(сторінка {page}/{pages}, > та < для перегортання)"),
        }
    }

    pub fn tui_hit(damage: u32, lethal: bool, locale: Locale) -> String {
        match locale {
            Locale::En => format!(
                "You hit the monster for {damage} damage!{}",
                if lethal { " It dies!" } else { "" }
            ),
            Locale::Uk => format!(
                "Ви влучаєте у чудовисько на {damage} шкоди!{}",
                if lethal { " Воно гине!" } else { "" }
            ),
        }
    }

    pub fn door_state(state: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("The door is now {state}."),
            Locale::Uk => format!("Стан дверей: {state}."),
        }
    }

    pub fn level_enter(depth: impl std::fmt::Display, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You enter dungeon level {depth}."),
            Locale::Uk => format!("Ви переходите на рівень {depth}."),
        }
    }

    pub fn attack_hit(attacker: &str, target: &str, damage: u32, locale: Locale) -> String {
        let attacker = tn(attacker, locale);
        let target = tn(target, locale);
        match locale {
            Locale::En => format!("{attacker} hits {target} for {damage} damage!"),
            Locale::Uk => format!("{attacker} влучає у {target} на {damage} шкоди!"),
        }
    }

    pub fn attack_miss(attacker: &str, target: &str, locale: Locale) -> String {
        let attacker = tn(attacker, locale);
        let target = tn(target, locale);
        match locale {
            Locale::En => format!("{attacker} misses {target}."),
            Locale::Uk => format!("{attacker} промахується повз {target}."),
        }
    }

    pub fn killed(target: &str, locale: Locale) -> String {
        let target = tn(target, locale);
        match locale {
            Locale::En => format!("{target} is killed!"),
            Locale::Uk => format!("{target} гине!"),
        }
    }

    /// C `setmangry` (mon.c:4306): "%s gets angry!" with `Monnam`.
    pub fn gets_angry(target: &str, locale: Locale) -> String {
        let target = capitalize_first(&tn(target, locale));
        match locale {
            Locale::En => format!("{target} gets angry!"),
            Locale::Uk => format!("{target} сердиться!"),
        }
    }

    /// C `setmangry` (mon.c:4271): attacking from an Elbereth square.
    pub fn feel_hypocrite(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You feel like a hypocrite.",
            Locale::Uk => "Ви почуваєтеся лицеміром.",
        }
    }

    /// C `setmangry` (mon.c:4283): the Elbereth under the hero is erased.
    pub fn engraving_fades(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The engraving beneath you fades.",
            Locale::Uk => "Напис під вами тьмяніє й зникає.",
        }
    }

    /// C `qst_guardians_respond` (mon.c:4156-4157): "The %s appear(s) to be angry too...".
    pub fn guardians_angry_too(guardian: &str, plural: bool, locale: Locale) -> String {
        let name = tn(guardian, locale);
        match (locale, plural) {
            (Locale::En, false) => format!("The {name} appears to be angry too..."),
            (Locale::En, true) => format!("The {name}s appear to be angry too..."),
            (Locale::Uk, false) => format!("Схоже, {name} теж сердиться..."),
            (Locale::Uk, true) => format!("Схоже, охоронці ({name}) теж сердяться..."),
        }
    }

    /// C `do_attack` (uhitm.c:500): "You stop.  %s is in the way!" (`y_monnam`).
    pub fn peaceful_in_the_way(target: &str, locale: Locale) -> String {
        let target = capitalize_first(&tn(target, locale));
        match locale {
            Locale::En => format!("You stop. {target} is in the way!"),
            Locale::Uk => format!("Ви зупиняєтеся. {target} заважає пройти!"),
        }
    }

    /// C `domove_swap_with_pet` (hack.c:2160): "You stop.  %s doesn't want to swap places."
    pub fn peaceful_wont_swap(target: &str, locale: Locale) -> String {
        let target = capitalize_first(&tn(target, locale));
        match locale {
            Locale::En => format!("You stop. {target} doesn't want to swap places."),
            Locale::Uk => format!("Ви зупиняєтеся. {target} не хоче мінятися місцями."),
        }
    }

    /// C `domove_swap_with_pet` (hack.c:2169): "You swap places with the peaceful %s."
    pub fn swap_with_peaceful(target: &str, locale: Locale) -> String {
        let target = tn(target, locale);
        match locale {
            Locale::En => format!("You swap places with the peaceful {target}."),
            Locale::Uk => format!("Ви міняєтеся місцями з мирною істотою: {target}."),
        }
    }

    pub fn vorpal_decapitate(target: &str, locale: Locale) -> String {
        let target = tn(target, locale);
        match locale {
            Locale::En => format!("*SNICKER-SNACK!* Vorpal Blade decapitates {target}!"),
            Locale::Uk => format!("*ХНИК-СНИК!* Гостросічний меч стинає голову {target}!"),
        }
    }

    pub fn drawbridge_collapse(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The striking ray shatters the drawbridge! It collapses into the moat!",
            Locale::Uk => "Промінь удару трощить підйомний міст! Він падає у рів з водою!",
        }
    }

    pub fn door_splinters(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The door splinters apart violently!",
            Locale::Uk => "Двері розлітаються на друзки!",
        }
    }

    pub fn pool_frozen(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The ray of frost freezes the water into a sheet of solid ice!",
            Locale::Uk => "Промінь холоду перетворює воду на шар міцного льоду!",
        }
    }

    pub fn secret_doors_found(count: usize, locale: Locale) -> String {
        match locale {
            Locale::En => format!("The wand tingles! {count} hidden door(s) are revealed!"),
            Locale::Uk => format!("Жезл вібрує! Виявлено {count} потаємних дверей!"),
        }
    }

    pub fn wand_empty(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Nothing happens. The wand is empty!",
            Locale::Uk => "Нічого не відбувається. Жезл порожній!",
        }
    }

    pub fn wand_recharged(name: &str, charges: u32, recharges: u32, locale: Locale) -> String {
        let name = tn(name, locale);
        match locale {
            Locale::En => format!(
                "Your {name} glows with bright light! Recharged to ({charges}:{recharges})!"
            ),
            Locale::Uk => format!(
                "Ваш {name} сяє яскравим світлом! Перезаряджено до ({charges}:{recharges})!"
            ),
        }
    }

    pub fn wand_exploded(name: &str, locale: Locale) -> String {
        let name = tn(name, locale);
        match locale {
            Locale::En => {
                format!("Your {name} vibrates violently and explodes in a blast of shards!")
            }
            Locale::Uk => format!("Ваш {name} шалено вібрує й вибухає смертоносними уламками!"),
        }
    }

    pub fn wish_granted(item_name: &str, locale: Locale) -> String {
        let item_name = tn(item_name, locale);
        match locale {
            Locale::En => {
                format!("A {item_name} miraculously drops from the heavens at your feet!")
            }
            Locale::Uk => {
                format!("Дивовижним чином {item_name} падає з небес просто до ваших ніг!")
            }
        }
    }

    pub fn wish_empty(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The wand of wishing is empty! Nothing happens.",
            Locale::Uk => "Жезл бажань порожній! Нічого не відбувається.",
        }
    }

    pub fn holy_water_consecrated(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "A flash of divine light consecrates your water into Holy Water!",
            Locale::Uk => {
                "Спалах божественного світла освячує вашу воду, перетворюючи її на Святу Воду!"
            }
        }
    }

    pub fn prayer_timeout(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You pray too soon! A cloud of brimstone appears, and you are struck by celestial lightning!",
            Locale::Uk => "Ви молитеся занадто рано! З'являється хмара сірки, і вас вражає небесна блискавка!",
        }
    }

    pub fn prayer_coaligned(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You feel devoutly reconciled with your god! You are fully healed.",
            Locale::Uk => {
                "Ви відчуваєте благоговійне примирення зі своїм божеством! Ви повністю зцілені."
            }
        }
    }

    pub fn prayer_neutral(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You feel a soothing warmth envelop you. You recover health.",
            Locale::Uk => {
                "Ви відчуваєте, як вас огортає заспокійливе тепло. Здоров'я відновлюється."
            }
        }
    }

    pub fn prayer_wrath(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The altar shudders violently! A voice thunders: 'Infidel!' Divine lightning strikes you!",
            Locale::Uk => "Вівтар шалено здригається! Громовий голос гукає: «Невірний!» Вас вражає божественна блискавка!",
        }
    }

    pub fn prayer_high_altar_coaligned(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You kneel before the High Altar of your deity on the Astral Plane. The presence of divinity hums with eternal power.",
            Locale::Uk => "Ви стаєте на коліна перед Високим Вівтарем вашого божества на Астральному Плані. Присутність божества пульсує вічною силою.",
        }
    }

    pub fn prayer_high_altar_foreign(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You sense intense celestial fury radiating from this foreign High Altar!",
            Locale::Uk => "Ви відчуваєте шалену небесну лють, що випромінюється від цього чужого Високого Вівтаря!",
        }
    }

    pub fn prayer_general(locale: Locale) -> &'static str {
        match locale {
            Locale::En => {
                "You pray to the gods of the dungeon. A harmonious chime echoes in the distance."
            }
            Locale::Uk => {
                "Ви молитеся до богів підземелля. Вдалині відлунює гармонійний передзвін."
            }
        }
    }

    pub fn altar_converted(item_name: &str, new_align: Alignment, locale: Locale) -> String {
        let item_name = tn(item_name, locale);
        let align_name = t_align(new_align, locale);
        match locale {
            Locale::En => format!("You sacrifice the {item_name}. An astral flash erupts and the altar converts to {align_name}!"),
            Locale::Uk => format!("Ви приносите в жертву {item_name}. Спалахує астральний спалах, і вівтар навертається у {align_name}!"),
        }
    }

    pub fn sacrifice_favor_increased(item_name: &str, favor: i32, locale: Locale) -> String {
        let item_name = tn(item_name, locale);
        match locale {
            Locale::En => format!("You sacrifice the {item_name}. An aura of divine light envelops the altar. You feel favored by your god (Favor: {favor}, +1 Max HP)!"),
            Locale::Uk => format!("Ви приносите в жертву {item_name}. Аура божественного сяйва огортає вівтар. Ви відчуваєте милість свого бога (Милість: {favor}, +1 Макс Здоров'я)!"),
        }
    }

    pub fn divine_crowning(gift: &str, locale: Locale) -> String {
        let gift = tn(gift, locale);
        match locale {
            Locale::En => format!("A thunderous celestial horn sounds! Your deity crowns you their champion and gifts you {gift}!"),
            Locale::Uk => format!("Лунає громовий небесний ріг! Ваше божество вінчає вас своїм чемпіоном і дарує {gift}!"),
        }
    }

    pub fn ascension_victory(god_align: Alignment, locale: Locale) -> String {
        let align_str = t_align(god_align, locale);
        match locale {
            Locale::En => format!("An astral choir erupts! You offer the Amulet of Yendor on your co-aligned {align_str} High Altar and ascend to immortality as a demigod!"),
            Locale::Uk => format!("Астральний хор вибухає співом! Ви підносите Амулет Єндора на свій {align_str} Високий Вівтар і підноситеся до безсмертя напівбогом!"),
        }
    }

    pub fn ascension_rejected(reason: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Offering rejected! {reason}"),
            Locale::Uk => format!("Жертвопринесення відхилено! {reason}"),
        }
    }

    pub fn high_altar_demands_amulet(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The High Altar demands nothing less than the genuine Amulet of Yendor!",
            Locale::Uk => "Високий Вівтар вимагає лише справжній Амулет Єндора!",
        }
    }

    pub fn no_altar(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "There is no altar here to sacrifice upon.",
            Locale::Uk => "Тут немає вівтаря для жертвопринесення.",
        }
    }

    pub fn no_sacrifice_item(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You don't have that item in your pack to sacrifice.",
            Locale::Uk => "У вас немає цього предмета в наплічнику для жертвопринесення.",
        }
    }

    pub fn hunger_fainting(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You are fainting from starvation!",
            Locale::Uk => "Ви непритомнієте від голоду!",
        }
    }

    pub fn hunger_hungry(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You are beginning to feel hungry.",
            Locale::Uk => "Ви починаєте відчувати голод.",
        }
    }

    pub fn hunger_weak(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You feel weak from hunger!",
            Locale::Uk => "Ви відчуваєте слабкість від голоду!",
        }
    }

    pub fn elbereth_repels(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The monster gazes upon the Elbereth engraving and flees in terror!",
            Locale::Uk => "Чудовисько бачить напис Елберет і в жаху втікає!",
        }
    }

    pub fn dragon_breath(monster: &str, breath_name: &str, damage: u32, locale: Locale) -> String {
        let monster = tn(monster, locale);
        match locale {
            Locale::En => format!(
                "{monster} breathes a fiery blast of {breath_name}! You take {damage} damage!"
            ),
            Locale::Uk => format!(
                "{monster} вивергає нищівний подих ({breath_name})! Ви отримуєте {damage} шкоди!"
            ),
        }
    }

    pub fn breath_reflected(monster: &str, locale: Locale) -> String {
        let monster = tn(monster, locale);
        match locale {
            Locale::En => {
                format!("Your reflection bounces the deadly breath back at the {monster}!")
            }
            Locale::Uk => format!("Ваше відбиття повертає смертоносний подих назад у {monster}!"),
        }
    }

    /// C `buzz` (zap.c:4984): the breath beam missed the hero (`zap_hit` failed).
    pub fn breath_misses(breath_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("The blast of {breath_name} whizzes by you!"),
            Locale::Uk => format!("Подих ({breath_name}) пролітає повз вас!"),
        }
    }

    pub fn breath_absorbed(breath_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You are engulfed in the blast of {breath_name}, but your innate resistance absorbs it completely!"),
            Locale::Uk => format!("Вас охоплює полум'яний подих ({breath_name}), але ваш вроджений опір повністю поглинає його!"),
        }
    }

    pub fn gaze_reflected(monster: &str, locale: Locale) -> String {
        let monster = tn(monster, locale);
        match locale {
            Locale::En => {
                format!("The {monster}'s terrifying gaze is reflected back into its own eyes!")
            }
            Locale::Uk => {
                format!("Жахливий погляд {monster} відбивається просто в його власні очі!")
            }
        }
    }

    pub fn gaze_afflicted(monster: &str, gaze_name: &str, locale: Locale) -> String {
        let monster = tn(monster, locale);
        match locale {
            Locale::En => {
                format!("You meet the gaze of the {monster}! You are struck by {gaze_name}!")
            }
            Locale::Uk => {
                format!("Ви зустрічаєтеся поглядом із {monster}! Вас охоплює {gaze_name}!")
            }
        }
    }

    pub fn monster_summon_incantation(monster: &str, count: usize, locale: Locale) -> String {
        let monster = tn(monster, locale);
        match locale {
            Locale::En => format!("The {monster} chants an unholy incantation! {count} creature(s) rise from the shadows!"),
            Locale::Uk => format!("{monster} вигукує темне заклинання! З тіні повстають {count} істот!"),
        }
    }

    pub fn monster_curse_item(item_name: &str, locale: Locale) -> String {
        let item_name = tn(item_name, locale);
        match locale {
            Locale::En => {
                format!("A malevolent aura sweeps over your pack! Your {item_name} is now cursed!")
            }
            Locale::Uk => format!(
                "Зловісна аура торкається вашого наплічника! Ваш {item_name} тепер проклятий!"
            ),
        }
    }

    pub fn ghost_encounter(hero_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You feel an icy chill down your spine... The vengeful ghost of {hero_name} haunts this graveyard!"),
            Locale::Uk => format!("Крижаний холод сковує ваш подих... Мстивий привид героя {hero_name} блукає цим могильником!"),
        }
    }

    pub fn potion_diluted(old_name: &str, new_name: &str, locale: Locale) -> String {
        let old_name = tn(old_name, locale);
        let new_name = tn(new_name, locale);
        match locale {
            Locale::En => {
                format!("You dip the {old_name} into the water. It becomes a {new_name}!")
            }
            Locale::Uk => {
                format!("Ви занурюєте {old_name} у воду. Зілля розбавляється і стає {new_name}!")
            }
        }
    }

    pub fn djinni_wishing(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "A majestic Djinni appears in a cloud of fragrant smoke! 'I am in your debt. Speak thy wish!'",
            Locale::Uk => "Величний Джин з'являється у хмарі запашного диму! 'Я твій боржник. Назви своє бажання!'",
        }
    }

    pub fn djinni_peaceful(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "A friendly Djinni emerges from the lamp, bows graciously, and vanishes into the ether.",
            Locale::Uk => "Дружній Джин виходить із лампи, ґречно вклоняється і розчиняється в ефірі.",
        }
    }

    pub fn djinni_hostile(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "An enraged Djinni bursts forth from the cursed lamp! 'Who dares disturb my eternal slumber?!'",
            Locale::Uk => "Оскаженілий Джин виривається з проклятої лампи! 'Хто посмів порушити мій вічний спокій?!'",
        }
    }

    pub fn lamp_smoke(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "A small puff of smoke curls up from the lamp, but nothing happens.",
            Locale::Uk => "Легкий струмок диму здіймається з лампи, але нічого не відбувається.",
        }
    }

    pub fn price_appraisal(
        item_name: &str,
        sell_price: u32,
        buy_price: u32,
        base: u32,
        locale: Locale,
    ) -> String {
        let item_name = tn(item_name, locale);
        match locale {
            Locale::En => format!(
                "The shopkeeper appraises your {item_name}: 'I'll give you {sell_price} zm for it, or sell it for {buy_price} zm.' (Base value: ~{base} zm)"
            ),
            Locale::Uk => format!(
                "Крамар оцінює ваш {item_name}: 'Я можу дати вам {sell_price} зм, або продати за {buy_price} зм.' (Базова вартість: ~{base} зм)"
            ),
        }
    }

    pub fn pet_whimpers_at_cursed(pet_name: &str, locale: Locale) -> String {
        let pet_name = tn(pet_name, locale);
        match locale {
            Locale::En => {
                format!("Your {pet_name} sniffs the ground and cautiously backs away, whimpering.")
            }
            Locale::Uk => format!(
                "Ваш {pet_name} обнюхує землю та обережно сахається назад, жалібно скавулячи."
            ),
        }
    }

    pub fn pet_grows(pet_name: &str, new_species: &str, locale: Locale) -> String {
        let pet_name = tn(pet_name, locale);
        let new_species = tn(new_species, locale);
        match locale {
            Locale::En => {
                format!("Your {pet_name} shimmers with vitality and grows into a {new_species}!")
            }
            Locale::Uk => {
                format!("Ваш {pet_name} сповнюється дикою силою та виростає у {new_species}!")
            }
        }
    }

    pub fn pet_defends_hero(pet_name: &str, hostile_name: &str, locale: Locale) -> String {
        let pet_name = tn(pet_name, locale);
        let hostile_name = tn(hostile_name, locale);
        match locale {
            Locale::En => {
                format!("Your {pet_name} fiercely attacks {hostile_name} to protect you!")
            }
            Locale::Uk => format!("Ваш {pet_name} люто атакує {hostile_name}, захищаючи вас!"),
        }
    }

    pub fn quest_leader_accept(
        leader_name: &str,
        artifact_name: &str,
        nemesis_name: &str,
        locale: Locale,
    ) -> String {
        let leader_name = tn(leader_name, locale);
        let artifact_name = tn(artifact_name, locale);
        let nemesis_name = tn(nemesis_name, locale);
        match locale {
            Locale::En => format!("{leader_name}: 'You have proven your devotion! Seek out {nemesis_name}, defeat them in their lair, and recover {artifact_name}!'"),
            Locale::Uk => format!("{leader_name}: 'Ти довів свою гідність! Знайди {nemesis_name}, подолай їх у лігві та поверни {artifact_name}!'"),
        }
    }

    pub fn quest_leader_reject_level(leader_name: &str, min_level: u32, locale: Locale) -> String {
        let leader_name = tn(leader_name, locale);
        match locale {
            Locale::En => format!("{leader_name}: 'You are not yet experienced enough for the trial. Return when you have achieved level {min_level}.'"),
            Locale::Uk => format!("{leader_name}: 'Твій досвід ще занадто малий для цього випробування. Повертайся, коли досягнеш {min_level} рівня.'"),
        }
    }

    pub fn quest_nemesis_defeat(nemesis_name: &str, artifact_name: &str, locale: Locale) -> String {
        let nemesis_name = tn(nemesis_name, locale);
        let artifact_name = tn(artifact_name, locale);
        match locale {
            Locale::En => format!("{nemesis_name} collapses with a death rattle! {artifact_name} clatters to the floor!"),
            Locale::Uk => format!("{nemesis_name} падає зі смертельним стогоном! {artifact_name} падає на кам'яну підлогу!"),
        }
    }

    pub fn quest_completed(leader_name: &str, artifact_name: &str, locale: Locale) -> String {
        let leader_name = tn(leader_name, locale);
        let artifact_name = tn(artifact_name, locale);
        match locale {
            Locale::En => format!("{leader_name}: 'Magnificent! You have brought back {artifact_name}! The gods bless your sacred ascension!'"),
            Locale::Uk => format!("{leader_name}: 'Чудово! Ти повернув {artifact_name}! Боги благословляють твоє священне сходження!'"),
        }
    }

    pub fn bump_wall(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Ouch! You bump into a solid wall.",
            Locale::Uk => "Ой! Ви врізаєтесь у глуху стіну.",
        }
    }

    pub fn shopkeeper_shout(locale: Locale) -> &'static str {
        match locale {
            Locale::En => {
                "You hear the shopkeeper shout: 'Stop, thief! You haven't paid for that!'"
            }
            Locale::Uk => "Ви чуєте крик крамаря: 'Стій, злодію! Ти не заплатив за це!'",
        }
    }

    pub fn nothing_to_pickup(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "There is nothing here to pick up.",
            Locale::Uk => "Тут немає нічого, що можна підібрати.",
        }
    }

    pub fn search_nothing(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You search the area but find nothing.",
            Locale::Uk => "Ви оглядаєтеся навколо, але нічого не знаходите.",
        }
    }

    pub fn trap_disarmed(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You successfully disarmed the trap.",
            Locale::Uk => "Вам вдалося успішно знешкодити пастку.",
        }
    }

    pub fn no_trap(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "There is no trap here to disarm.",
            Locale::Uk => "Тут немає пастки для знешкодження.",
        }
    }

    pub fn quiver_success(item_name: &str, locale: Locale) -> String {
        let item_name = tn(item_name, locale);
        match locale {
            Locale::En => format!("You ready {item_name} in your quiver."),
            Locale::Uk => format!("Ви вклали {item_name} у сагайдак."),
        }
    }

    pub fn quiver_empty(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You have nothing quivered.",
            Locale::Uk => "У вашому сагайдаку порожньо.",
        }
    }

    pub fn petrification_death(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You turn to stone...",
            Locale::Uk => "Ви перетворюєтесь на холодний камінь...",
        }
    }

    pub fn sliming_death(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You turn into green slime...",
            Locale::Uk => "Ви перетворюєтесь на зеленого слимака...",
        }
    }

    pub fn pickup_item(name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You pick up a {name}."),
            Locale::Uk => format!("Ви підбираєте {}.", tn(name, locale)),
        }
    }

    pub fn no_stairs_down(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "There are no stairs leading down here.",
            Locale::Uk => "Тут немає сходів, що ведуть вниз.",
        }
    }

    pub fn no_stairs_up(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "There are no stairs leading up here.",
            Locale::Uk => "Тут немає сходів, що ведуть вгору.",
        }
    }

    pub fn mount_steed(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You mount the steed.",
            Locale::Uk => "Ви сідаєте на скакуна.",
        }
    }

    pub fn cannot_mount(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You cannot mount this monster.",
            Locale::Uk => "Ви не можете осідлати цю істоту.",
        }
    }

    pub fn dismount_steed(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You dismount.",
            Locale::Uk => "Ви спішуєтесь.",
        }
    }

    pub fn not_mounted(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You are not mounted.",
            Locale::Uk => "Ви не верхи.",
        }
    }

    pub fn revert_form(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You revert to your normal form!",
            Locale::Uk => "Ви повертаєтесь до свого звичайного вигляду!",
        }
    }

    pub fn hit_monster(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You hit a monster!",
            Locale::Uk => "Ви влучили у монстра!",
        }
    }

    pub fn projectile_breaks(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The projectile breaks!",
            Locale::Uk => "Снаряд розбивається вщент!",
        }
    }

    pub fn projectile_misses(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The projectile misses and lands on the floor.",
            Locale::Uk => "Снаряд пролітає повз та падає на підлогу.",
        }
    }

    pub fn slime_burned(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "The fire burns away the slime!",
            Locale::Uk => "Вогонь випалює слиз!",
        }
    }
}

/// Translates item names to the specified locale.
pub fn t_item(name: &str, locale: Locale) -> String {
    if locale == Locale::En {
        return name.to_string();
    }
    match name.to_lowercase().as_str() {
        "long sword" => "довгий меч".into(),
        "short sword" => "короткий меч".into(),
        "dagger" => "кинджал".into(),
        "bow" => "лук".into(),
        "arrow" => "стріла".into(),
        "crossbow" => "арбалет".into(),
        "crossbow bolt" => "арбалетний болт".into(),
        "club" => "кийок".into(),
        "shield" => "щит".into(),
        "small shield" => "малий щит".into(),
        "plate mail" => "латний обладунок".into(),
        "leather armor" => "шкіряна броня".into(),
        "potion of healing" => "зілля зцілення".into(),
        "potion of extra healing" => "зілля сильного зцілення".into(),
        "potion of water" => "зілля води".into(),
        "potion of holy water" => "зілля святої води".into(),
        "potion of unholy water" => "зілля нечестивої води".into(),
        "potion of polymorph" => "зілля поліморфізму".into(),
        "potion of acid" => "зілля кислоти".into(),
        "scroll of teleportation" => "сувій телепортації".into(),
        "scroll of identify" => "сувій розпізнання".into(),
        "scroll of remove curse" => "сувій зняття прокляття".into(),
        "scroll of enchant weapon" => "сувій чарування зброї".into(),
        "scroll of enchant armor" => "сувій чарування броні".into(),
        "scroll of genocide" => "сувій геноциду".into(),
        "wand of striking" => "жезл удару".into(),
        "wand of cold" => "жезл холоду".into(),
        "wand of fire" => "жезл вогню".into(),
        "wand of sleep" => "жезл сну".into(),
        "wand of death" => "жезл смерті".into(),
        "wand of digging" => "жезл копання".into(),
        "wand of wishing" => "жезл бажань".into(),
        "wand of polymorph" => "жезл поліморфізму".into(),
        "magic marker" => "чарівний маркер".into(),
        "food ration" => "пайок".into(),
        "corpse" => "труп".into(),
        "lizard corpse" => "труп ящірки".into(),
        "amulet of yendor" => "Амулет Єндора".into(),
        "bell of opening" => "Дзвін Відкриття".into(),
        "candelabrum of invocation" => "Свічник Поклику".into(),
        "book of the dead" => "Книга Мертвих".into(),
        "silver saber" => "срібна шабля".into(),
        "mace" => "булава".into(),
        "chain mail" => "кольчуга".into(),
        "silver dragon scale mail" => "обладунок зі срібної драконячої луски".into(),
        "cloak of magic resistance" => "плащ опору магії".into(),
        "wand of teleportation" => "жезл телепортації".into(),
        "wand of secret door detection" => "жезл виявлення потайних дверей".into(),
        "potion of speed" => "зілля швидкості".into(),
        "scroll of charging" => "сувій заряджання".into(),
        "sack" => "мішок".into(),
        "bag of holding" => "чарівна торба".into(),
        "chest" => "скриня".into(),
        "magic lamp" => "чарівна лампа".into(),
        "oil lamp" => "олійна лампа".into(),
        "boulder" => "валун".into(),
        "apple" => "яблуко".into(),
        "spellbook of force bolt" => "книга заклять: силовий удар".into(),
        "spellbook of healing" => "книга заклять: зцілення".into(),
        "amulet of reflection" => "амулет відбиття".into(),
        "cheap plastic imitation of the amulet of yendor" => {
            "дешева пластикова імітація Амулета Єндора".into()
        }
        "excalibur" => "Екскалібур".into(),
        "vorpal blade" => "Гостросічний меч".into(),
        "mjollnir" => "Мйольнір".into(),
        "magicbane" => "Чарозгуба".into(),
        "the eye of the aethiopica" => "Око Етіопіки".into(),
        "gold pieces" => "золоті монети".into(),
        "luckstone" => "камінь удачі".into(),
        "wax candle" => "воскова свічка".into(),
        "the orb of fate" => "Сфера Долі".into(),
        "the heart of ahriman" => "Серце Аримана".into(),
        "the magic mirror of merlin" => "Чарівне дзеркало Мерліна".into(),
        "the eyes of the overworld" => "Очі Верхнього Світу".into(),
        "the master key of thievery" => "Відмичка Злодійства".into(),
        "the tsurugi of muramasa" => "Цуруґі Мурамаси".into(),
        "the platinum yendorian express card" => "Платинова картка «Єндор Експрес»".into(),
        "the staff of aesculapius" => "Посох Асклепія".into(),
        "the orb of detection" => "Сфера Виявлення".into(),
        "sprig of wolfsbane" => "гілочка аконіту".into(),
        _ => {
            let lower = name.to_lowercase();
            if let Some(stripped) = lower.strip_suffix("corpse") {
                let inner = stripped.trim();
                format!("труп ({})", t_monster(inner, locale))
            } else {
                name.to_string()
            }
        }
    }
}

/// Translates monster names to the specified locale.
pub fn t_monster(name: &str, locale: Locale) -> String {
    if locale == Locale::En {
        return name.to_string();
    }
    match name.to_lowercase().as_str() {
        "hill orc" => "гірський орк".into(),
        "orc" => "орк".into(),
        "goblin" => "гоблін".into(),
        "jackal" => "шакал".into(),
        "newt" => "тритон".into(),
        "kobold" => "кобольд".into(),
        "cockatrice" => "кокатрис".into(),
        "green slime" => "зелений слиз".into(),
        "floating eye" => "летюче око".into(),
        "red dragon" => "червоний дракон".into(),
        "lich" => "ліч".into(),
        "shopkeeper" => "крамар".into(),
        "priest" => "жрець".into(),
        "medusa" => "Медуза".into(),
        "vlad the impaler" => "Влад Цепеш".into(),
        "wizard of yendor" => "Чарівник Єндора".into(),
        "croesus" => "Крез".into(),
        "hobgoblin" => "гобгоблін".into(),
        "giant ant" => "гігантський мураха".into(),
        "skeleton" => "скелет".into(),
        "vampire" => "вампір".into(),
        "silver dragon" => "срібний дракон".into(),
        "master lich" => "верховний ліч".into(),
        "little dog" => "песик".into(),
        "dog" => "пес".into(),
        "large dog" => "великий пес".into(),
        "kitten" => "кошеня".into(),
        "housecat" => "домашній кіт".into(),
        "large cat" => "великий кіт".into(),
        "ghost" => "привид".into(),
        "djinni" => "джин".into(),
        "gnome" => "гном".into(),
        "dwarf" => "дворф".into(),
        "watchman" => "вартовий".into(),
        "the norn" => "Норна".into(),
        "neferet the green" => "Неферет Зелена".into(),
        "pelias" => "Пелій".into(),
        "king arthur" => "король Артур".into(),
        "grand master" => "Великий Майстер".into(),
        "master assassin" => "Майстер-асасин".into(),
        "hippocrates" => "Гіппократ".into(),
        "twoflower" => "Двоквіт".into(),
        "lord carnarvon" => "лорд Карнарвон".into(),
        "lord surtur" => "лорд Сурт".into(),
        "the dark one" => "Темний".into(),
        "thoth amon" => "Тот Амон".into(),
        "ixoth" => "Іксот".into(),
        "master kaen" => "майстер Каен".into(),
        "master of thieves" => "Майстер Злодіїв".into(),
        "cyclops" => "Циклоп".into(),
        "minion of huhetotl" => "Слуга Хухетотля".into(),
        "student" => "студент".into(),
        "chieftain" => "вождь".into(),
        "attendant" => "санітар".into(),
        "page" => "паж".into(),
        "abbot" => "абат".into(),
        "thug" => "головоріз".into(),
        "guide" => "гід".into(),
        "warrior" => "воїн".into(),
        "apprentice" => "учень".into(),
        _ => {
            if let Some(rest) = name.strip_prefix("ghost of ") {
                format!("привид героя {rest}")
            } else {
                name.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_ui_strings() {
        assert_eq!(t("hero", Locale::En), "Hero");
        assert_eq!(t("hero", Locale::Uk), "Герой");
        assert_eq!(t("gold", Locale::En), "Gold");
        assert_eq!(t("gold", Locale::Uk), "Золото");
        assert_eq!(t("descend", Locale::Uk), "Вниз");
    }

    #[test]
    fn test_i18n_align_and_buc() {
        assert_eq!(t_align(Alignment::Lawful, Locale::En), "Lawful");
        assert_eq!(t_align(Alignment::Lawful, Locale::Uk), "Законний");
        assert_eq!(t_buc(Buc::Blessed, Locale::En), "blessed");
        assert_eq!(t_buc(Buc::Blessed, Locale::Uk), "благословенний");
        assert_eq!(
            t_branch(BranchId::AstralPlane, Locale::Uk),
            "Астральний План"
        );
    }

    #[test]
    fn test_i18n_messages() {
        assert!(Messages::welcome(Locale::Uk).contains("Lean 4"));
        assert!(Messages::attack_hit("Варвар", "гоблін", 12, Locale::Uk).contains("12 шкоди"));
        assert!(Messages::drawbridge_collapse(Locale::Uk).contains("підйомний міст"));
        assert!(Messages::vorpal_decapitate("Дракон", Locale::Uk).contains("Гостросічний меч"));
        assert!(Messages::ascension_victory(Alignment::Neutral, Locale::Uk).contains("безсмертя"));
        assert!(Messages::prayer_timeout(Locale::Uk).contains("блискавка"));
        assert!(Messages::prayer_coaligned(Locale::Uk).contains("примирення"));
        assert_eq!(
            Messages::bump_wall(Locale::Uk),
            "Ой! Ви врізаєтесь у глуху стіну."
        );
        assert_eq!(
            Messages::nothing_to_pickup(Locale::Uk),
            "Тут немає нічого, що можна підібрати."
        );
        assert_eq!(
            Messages::no_stairs_down(Locale::Uk),
            "Тут немає сходів, що ведуть вниз."
        );
        assert_eq!(
            Messages::no_stairs_up(Locale::Uk),
            "Тут немає сходів, що ведуть вгору."
        );
        assert_eq!(
            Messages::revert_form(Locale::Uk),
            "Ви повертаєтесь до свого звичайного вигляду!"
        );
    }

    #[test]
    fn test_i18n_item_and_monster_translation() {
        assert_eq!(t_item("long sword", Locale::Uk), "довгий меч");
        assert_eq!(t_item("long sword", Locale::En), "long sword");
        assert_eq!(t_item("dagger", Locale::Uk), "кинджал");
        assert_eq!(
            t_item("potion of holy water", Locale::Uk),
            "зілля святої води"
        );
        assert_eq!(t_item("scroll of genocide", Locale::Uk), "сувій геноциду");
        assert_eq!(t_item("wand of wishing", Locale::Uk), "жезл бажань");
        assert_eq!(t_item("orc corpse", Locale::Uk), "труп (орк)");

        assert_eq!(t_monster("orc", Locale::Uk), "орк");
        assert_eq!(t_monster("orc", Locale::En), "orc");
        assert_eq!(t_monster("red dragon", Locale::Uk), "червоний дракон");
        assert_eq!(t_monster("shopkeeper", Locale::Uk), "крамар");
        assert_eq!(t_monster("wizard of yendor", Locale::Uk), "Чарівник Єндора");
    }
}
