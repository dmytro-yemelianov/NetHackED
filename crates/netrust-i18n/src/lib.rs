//! Complete Internationalization (i18n) Engine for NetRust.
//!
//! Provides first-class English (canonical NetHack) and Ukrainian (uk-UA)
//! localized ontologies, UI strings, message formatters, and status descriptions.

pub use netrust_types::Locale;
use netrust_types::{Alignment, BranchId, Buc};

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

/// Message formatters with full locale support.
pub struct Messages;

impl Messages {
    pub fn welcome(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Welcome to NetRust! Formalized and mathematically verified in Lean 4.",
            Locale::Uk => "Ласкаво просимо до NetRust! Формалізовано та математично доведено в Lean 4.",
        }
    }

    pub fn attack_hit(attacker: &str, target: &str, damage: u32, locale: Locale) -> String {
        match locale {
            Locale::En => format!("{} hits {} for {} damage!", attacker, target, damage),
            Locale::Uk => format!("{} влучає у {} на {} шкоди!", attacker, target, damage),
        }
    }

    pub fn attack_miss(attacker: &str, target: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("{} misses {}.", attacker, target),
            Locale::Uk => format!("{} промахується повз {}.", attacker, target),
        }
    }

    pub fn killed(target: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("{} is killed!", target),
            Locale::Uk => format!("{} гине!", target),
        }
    }

    pub fn vorpal_decapitate(target: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("*SNICKER-SNACK!* Vorpal Blade decapitates {}!", target),
            Locale::Uk => format!("*ХНИК-СНИК!* Гостросічний меч стинає голову {}!", target),
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
            Locale::En => format!("The wand tingles! {} hidden door(s) are revealed!", count),
            Locale::Uk => format!("Жезл вібрує! Виявлено {} потаємних дверей!", count),
        }
    }

    pub fn wand_empty(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Nothing happens. The wand is empty!",
            Locale::Uk => "Нічого не відбувається. Жезл порожній!",
        }
    }

    pub fn wand_recharged(name: &str, charges: u32, recharges: u32, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Your {} glows with bright light! Recharged to ({}:{})!", name, charges, recharges),
            Locale::Uk => format!("Ваш {} сяє яскравим світлом! Перезаряджено до ({}:{})!", name, charges, recharges),
        }
    }

    pub fn wand_exploded(name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Your {} vibrates violently and explodes in a blast of shards!", name),
            Locale::Uk => format!("Ваш {} шалено вібрує й вибухає смертоносними уламками!", name),
        }
    }

    pub fn wish_granted(item_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("A {} miraculously drops from the heavens at your feet!", item_name),
            Locale::Uk => format!("Дивовижним чином {} падає з небес просто до ваших ніг!", item_name),
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
            Locale::Uk => "Спалах божественного світла освячує вашу воду, перетворюючи її на Святу Воду!",
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
            Locale::Uk => "Ви відчуваєте благоговійне примирення зі своїм божеством! Ви повністю зцілені.",
        }
    }

    pub fn prayer_neutral(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "You feel a soothing warmth envelop you. You recover health.",
            Locale::Uk => "Ви відчуваєте, як вас огортає заспокійливе тепло. Здоров'я відновлюється.",
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
            Locale::En => "You pray to the gods of the dungeon. A harmonious chime echoes in the distance.",
            Locale::Uk => "Ви молитеся до богів підземелля. Вдалині відлунює гармонійний передзвін.",
        }
    }

    pub fn altar_converted(item_name: &str, new_align: Alignment, locale: Locale) -> String {
        let align_name = t_align(new_align, locale);
        match locale {
            Locale::En => format!("You sacrifice the {}. An astral flash erupts and the altar converts to {}!", item_name, align_name),
            Locale::Uk => format!("Ви приносите в жертву {}. Спалахує астральний спалах, і вівтар навертається у {}!", item_name, align_name),
        }
    }

    pub fn sacrifice_favor_increased(item_name: &str, favor: i32, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You sacrifice the {}. An aura of divine light envelops the altar. You feel favored by your god (Favor: {}, +1 Max HP)!", item_name, favor),
            Locale::Uk => format!("Ви приносите в жертву {}. Аура божественного сяйва огортає вівтар. Ви відчуваєте милість свого бога (Милість: {}, +1 Макс Здоров'я)!", item_name, favor),
        }
    }

    pub fn divine_crowning(gift: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("A thunderous celestial horn sounds! Your deity crowns you their champion and gifts you {}!", gift),
            Locale::Uk => format!("Лунає громовий небесний ріг! Ваше божество вінчає вас своїм чемпіоном і дарує {}!", gift),
        }
    }

    pub fn ascension_victory(god_align: Alignment, locale: Locale) -> String {
        let align_str = t_align(god_align, locale);
        match locale {
            Locale::En => format!("An astral choir erupts! You offer the Amulet of Yendor on your co-aligned {} High Altar and ascend to immortality as a demigod!", align_str),
            Locale::Uk => format!("Астральний хор вибухає співом! Ви підносите Амулет Єндора на свій {} Високий Вівтар і підноситеся до безсмертя напівбогом!", align_str),
        }
    }

    pub fn ascension_rejected(reason: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Offering rejected! {}", reason),
            Locale::Uk => format!("Жертвопринесення відхилено! {}", reason),
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
        match locale {
            Locale::En => format!("{} breathes a fiery blast of {}! You take {} damage!", monster, breath_name, damage),
            Locale::Uk => format!("{} вивергає нищівний подих ({})! Ви отримуєте {} шкоди!", monster, breath_name, damage),
        }
    }

    pub fn breath_reflected(monster: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("Your reflection bounces the deadly breath back at the {}!", monster),
            Locale::Uk => format!("Ваше відбиття повертає смертоносний подих назад у {}!", monster),
        }
    }

    pub fn breath_absorbed(breath_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You are engulfed in the blast of {}, but your innate resistance absorbs it completely!", breath_name),
            Locale::Uk => format!("Вас охоплює полум'яний подих ({}), але ваш вроджений опір повністю поглинає його!", breath_name),
        }
    }

    pub fn gaze_reflected(monster: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("The {}'s terrifying gaze is reflected back into its own eyes!", monster),
            Locale::Uk => format!("Жахливий погляд {} відбивається просто в його власні очі!", monster),
        }
    }

    pub fn gaze_afflicted(monster: &str, gaze_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("You meet the gaze of the {}! You are struck by {}!", monster, gaze_name),
            Locale::Uk => format!("Ви зустрічаєтеся поглядом із {}! Вас охоплює {}!", monster, gaze_name),
        }
    }

    pub fn monster_summon_incantation(monster: &str, count: usize, locale: Locale) -> String {
        match locale {
            Locale::En => format!("The {} chants an unholy incantation! {} creature(s) rise from the shadows!", monster, count),
            Locale::Uk => format!("{} вигукує темне заклинання! З тіні повстають {} істот!", monster, count),
        }
    }

    pub fn monster_curse_item(item_name: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!("A malevolent aura sweeps over your pack! Your {} is now cursed!", item_name),
            Locale::Uk => format!("Зловісна аура торкається вашого наплічника! Ваш {} тепер проклятий!", item_name),
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
        assert_eq!(t_branch(BranchId::AstralPlane, Locale::Uk), "Астральний План");
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
    }
}
