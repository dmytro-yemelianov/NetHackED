//! Melee combat resolution and damage application.

use netrust_arena::{ActorId, ActorRecord, ItemLocation};
use netrust_core::{
    combat::{
        dmgval, is_melee_attack, mattacku_die, mhitm_to_hit, monster_attack_damage,
        monster_attack_hits, monster_hit_damage, monster_to_hit_value, resisted,
        resolve_melee_attack, to_hit_value, weapon_damage_die,
    },
    Combatant,
};
use netrust_data::{
    create_item_record, item_archetype_by_name, monster_archetype_by_name, ItemKindId, MonsterSize,
};
use netrust_types::{Attack, AttackType, Buc, DamageType};
use rand::{Rng, RngCore};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// Attack used for an actor whose name resolves to no bestiary archetype
/// (custom or renamed actors): a single `d(1, 6)` hand-to-hand hit, the
/// pre-D2 sim's one-d6 monster attack. Not a C rule; documented divergence.
pub(crate) const FALLBACK_ATTACK: Attack = Attack {
    at: AttackType::Claw,
    ad: DamageType::Phys,
    n: 1,
    d: 6,
};

impl SimulationWorld {
    pub(crate) fn resolve_combat(
        &mut self,
        attacker_id: ActorId,
        defender_id: ActorId,
    ) -> Vec<GameEvent> {
        if attacker_id != self.player_id {
            return self.resolve_monster_attacks(attacker_id, defender_id);
        }
        let mut events = Vec::new();
        let Some(attacker) = self.arena.actors.get(attacker_id).cloned() else {
            return events;
        };
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else {
            return events;
        };

        if defender.is_peaceful {
            if let Some(def_mut) = self.arena.actors.get_mut(defender_id) {
                def_mut.is_peaceful = false;
            }
            events.push(GameEvent::LogMessage {
                text: format!("{} turns hostile!", defender.name),
            });
        }

        let def_combat = Combatant {
            hp: defender.hp,
            max_hp: defender.max_hp,
            ac: self.defender_ac(defender_id, &defender),
            level: defender.level,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: defender.is_dead,
        };

        // Check attacker wielded weapon enchantment and signature artifact
        let (weapon_ench, artifact) = if attacker_id == self.player_id {
            if let Some(wid) = self.wielded_item {
                if let Some(w) = self.arena.items.get(wid) {
                    let art = match w.name.to_lowercase().as_str() {
                        "excalibur" => Some(netrust_types::ArtifactKind::Excalibur),
                        "vorpal blade" => Some(netrust_types::ArtifactKind::VorpalBlade),
                        "mjollnir" => Some(netrust_types::ArtifactKind::Mjollnir),
                        "magicbane" => Some(netrust_types::ArtifactKind::Magicbane),
                        "the eye of the aethiopica" | "eye of the aethiopica" => {
                            Some(netrust_types::ArtifactKind::EyeOfTheAethiopica)
                        }
                        _ => None,
                    };
                    (w.enchantment as i32, art)
                } else {
                    (0, None)
                }
            } else {
                (0, None)
            }
        } else {
            (0, None)
        };

        let mut skill_hit_bonus = 0;
        let mut skill_dmg_bonus = 0;
        if attacker_id == self.player_id {
            if let Some(wid) = self.wielded_item {
                if let Some(w) = self.arena.items.get(wid) {
                    let sk = match w.name.to_lowercase().as_str() {
                        n if n.contains("dagger") => Some(netrust_types::SkillClass::Dagger),
                        n if n.contains("long sword") => Some(netrust_types::SkillClass::LongSword),
                        n if n.contains("short sword") => {
                            Some(netrust_types::SkillClass::ShortSword)
                        }
                        n if n.contains("bow") => Some(netrust_types::SkillClass::Bow),
                        n if n.contains("crossbow") => Some(netrust_types::SkillClass::Crossbow),
                        n if n.contains("club") => Some(netrust_types::SkillClass::Club),
                        _ => None,
                    };
                    if let Some(skill_class) = sk {
                        let level = self
                            .hero
                            .skills
                            .skills
                            .get(&skill_class)
                            .copied()
                            .unwrap_or(netrust_types::SkillLevel::Unskilled);
                        skill_hit_bonus = netrust_core::skills::skill_to_hit_bonus(level);
                        skill_dmg_bonus = netrust_core::skills::skill_damage_bonus(level);
                    }
                }
            } else {
                let level = self
                    .hero
                    .skills
                    .skills
                    .get(&netrust_types::SkillClass::BareHanded)
                    .copied()
                    .unwrap_or(netrust_types::SkillLevel::Unskilled);
                // C weapon.c:1601: bare-handed uses its own table (+1/+2), not -4.
                skill_hit_bonus = netrust_core::skills::bare_handed_hit_bonus(level);
                skill_dmg_bonus = netrust_core::skills::skill_damage_bonus(level);
            }
        }

        let weapon_dice = if let Some(wid) = self.wielded_item {
            if let Some(w) = self.arena.items.get(wid) {
                if let Some(arch) = item_archetype_by_name(&w.name) {
                    if arch.damage_small.1 > 0 || arch.damage_large.1 > 0 {
                        Some((arch.damage_small.1, arch.damage_large.1))
                    } else {
                        // C uhitm.c:895: non-weapon object wielded as weapon deals rnd(2)
                        Some((2, 2))
                    }
                } else {
                    let lower = w.name.to_lowercase();
                    if lower.contains("dagger") {
                        Some((4, 3))
                    } else if lower.contains("short sword") {
                        Some((6, 8))
                    } else if lower.contains("long sword")
                        || lower.contains("excalibur")
                        || lower.contains("vorpal blade")
                    {
                        Some((8, 12))
                    } else if lower.contains("silver saber") {
                        Some((8, 8))
                    } else if lower.contains("mace") {
                        Some((6, 6))
                    } else {
                        Some((2, 2))
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        let target_large = monster_archetype_by_name(&defender.name)
            .map(|m| m.size >= MonsterSize::Large)
            .unwrap_or(false);
        let martial_arts = self.role_name.eq_ignore_ascii_case("monk");

        let target_ac = def_combat.ac;
        // Hero attacker: C find_roll_to_hit (uhitm.c:365) vs rnd(20) (uhitm.c:780).
        let to_hit = to_hit_value(
            attacker.level as i32,
            self.player_luck,
            weapon_ench,
            skill_hit_bonus,
            target_ac,
        );
        let d20 = self.rng.random_range(1..=20u32);
        let die = weapon_damage_die(weapon_dice, target_large, martial_arts);
        let roll = if die == 0 {
            0
        } else {
            self.rng.random_range(1..=die)
        };
        let base_roll = dmgval(weapon_dice, target_large, martial_arts, roll);

        let result = resolve_melee_attack(
            to_hit,
            def_combat,
            d20,
            base_roll,
            weapon_ench,
            skill_dmg_bonus,
            None,
        );

        if result.hit {
            let is_demon_or_undead = defender.name.to_lowercase().contains("demon")
                || defender.name.to_lowercase().contains("lich")
                || defender.name.to_lowercase().contains("vampire")
                || defender.name.to_lowercase().contains("zombie")
                || defender.name.to_lowercase().contains("skeleton");

            let mut final_damage = result.damage_dealt;
            if let Some(art) = artifact {
                final_damage = netrust_core::artifacts_wands::resolve_artifact_damage(
                    art,
                    final_damage,
                    is_demon_or_undead,
                );
            }

            let decap = if artifact == Some(netrust_types::ArtifactKind::VorpalBlade) {
                (self.rng.next_u32() % 20) == 0
            } else {
                false
            };
            if decap {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::vorpal_decapitate(&defender.name, self.locale),
                });
            }
            self.land_hit(
                attacker_id,
                &attacker,
                defender_id,
                &defender,
                final_damage,
                decap,
                &mut events,
            );
        } else {
            self.push_miss(attacker_id, &attacker, defender_id, &defender, &mut events);
        }

        events
    }

    /// Apply a landed hit's damage to the defender and emit the hit/kill events
    /// (shared by hero, monster and pet attacks). Returns `true` when lethal.
    #[allow(clippy::too_many_arguments)]
    fn land_hit(
        &mut self,
        attacker_id: ActorId,
        attacker: &ActorRecord,
        defender_id: ActorId,
        defender: &ActorRecord,
        final_damage: u32,
        decap: bool,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        let mut lethal = false;
        if let Some(target) = self.arena.actors.get_mut(defender_id) {
            if defender_id == self.player_id {
                let mut actual_damage = final_damage as i32;
                if decap {
                    actual_damage += 9999;
                } // Force fatal

                // Unchanging is not tracked on Hero yet; pass false.
                let poly_res = netrust_core::polymorph::apply_poly_damage(
                    &mut self.hero,
                    actual_damage,
                    false,
                );
                match poly_res {
                    netrust_core::polymorph::PolyDamageResult::Absorbed
                    | netrust_core::polymorph::PolyDamageResult::BaseDamaged => {
                        if let Some(poly) = &self.hero.polymorph {
                            target.hp = poly.hp as u32;
                            target.max_hp = poly.max_hp as u32;
                        } else {
                            target.hp = self.hero.base_hp as u32;
                            target.max_hp = self.hero.base_max_hp as u32;
                        }
                        lethal = false;
                    }
                    netrust_core::polymorph::PolyDamageResult::Reverted => {
                        target.hp = self.hero.base_hp as u32;
                        target.max_hp = self.hero.base_max_hp as u32;
                        lethal = false;
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::revert_form(self.locale).to_string(),
                        });
                    }
                    netrust_core::polymorph::PolyDamageResult::Dead => {
                        target.hp = 0;
                        target.is_dead = true;
                        lethal = true;
                    }
                }
            } else {
                let (new_hp, dead) = if decap {
                    netrust_core::artifacts_wands::apply_vorpal_strike(target.hp, decap)
                } else {
                    (
                        target.hp.saturating_sub(final_damage),
                        target.hp <= final_damage,
                    )
                };
                target.hp = new_hp;
                target.is_dead = dead || target.hp == 0;
                lethal = target.is_dead;
            }
        }
        events.push(GameEvent::AttackLanded {
            attacker: attacker_id,
            target: defender_id,
            damage: final_damage,
            lethal,
        });
        let attack_msg = netrust_i18n::Messages::attack_hit(
            &attacker.name,
            &defender.name,
            final_damage,
            self.locale,
        );
        events.push(GameEvent::LogMessage { text: attack_msg });
        if lethal {
            if attacker_id == self.player_id {
                netrust_core::conducts::record_kill(&mut self.conducts);
            }
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::killed(&defender.name, self.locale),
            });
            if defender_id != self.player_id {
                // Check if the defeated enemy is the unique Class Nemesis
                let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
                if defender.name.eq_ignore_ascii_case(quest_cfg.nemesis_name) {
                    netrust_core::attack_nemesis(&mut self.quest_state, 9999);
                    let art_id = match quest_cfg.role_name.to_lowercase().as_str() {
                        "valkyrie" => ItemKindId::OrbOfFate,
                        "wizard" => ItemKindId::EyeOfTheAethiopica,
                        "barbarian" => ItemKindId::HeartOfAhriman,
                        "knight" => ItemKindId::MagicMirrorOfMerlin,
                        "monk" => ItemKindId::EyesOfTheOverworld,
                        "rogue" => ItemKindId::MasterKeyOfThievery,
                        "tourist" => ItemKindId::PlatinumYendorianExpressCard,
                        "healer" => ItemKindId::StaffOfAesculapius,
                        _ => ItemKindId::OrbOfDetection,
                    };
                    let art_rec = create_item_record(
                        art_id,
                        ItemLocation::Floor(defender.coord),
                        Buc::Blessed,
                    );
                    self.arena.spawn_item(art_rec);
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::quest_nemesis_defeat(
                            quest_cfg.nemesis_name,
                            quest_cfg.artifact_name,
                            self.locale,
                        ),
                    });
                }

                let corpse = create_item_record(
                    ItemKindId::Corpse,
                    ItemLocation::Floor(defender.coord),
                    Buc::Uncursed,
                );
                self.arena.spawn_item(corpse);
            }
        }
        lethal
    }

    /// Defender AC as the sim models `find_mac`/`u.uac`.
    ///
    /// For the hero (`self.player_id`), `defender.ac` is maintained by
    /// [`SimulationWorld::recompute_hero_ac`]. For monsters, returns base AC minus
    /// the enchantment of carried armor (C `find_mac`, `worn.c:717`).
    pub(crate) fn defender_ac(&self, defender_id: ActorId, defender: &ActorRecord) -> i32 {
        if defender_id == self.player_id {
            defender.ac
        } else {
            let armor_ench: i32 = self
                .arena
                .items_carried_by(defender_id)
                .into_iter()
                .filter_map(|id| self.arena.items.get(id))
                .filter(|it| it.class == netrust_types::ItemClass::Armor)
                .map(|a| a.enchantment as i32)
                .sum();
            defender.ac - armor_ench
        }
    }

    /// A monster's full melee attack round against the hero (C `mattacku`,
    /// `mhitu.c:491`) or another monster (C `mattackm`, `mhitm.c:293`).
    ///
    /// Loops the archetype's `mattk[]` in slot order (`mhitu.c:768`,
    /// `mhitm.c:375`); only the adjacent hand-to-hand types
    /// ([`is_melee_attack`]) act here. Breath and gaze are handled as ranged
    /// abilities in `monsters.rs`; AT_MAGC spells by the spellcaster abilities;
    /// AT_NONE is passive. An actor whose name has no archetype falls back to
    /// [`FALLBACK_ATTACK`].
    ///
    /// RNG draws, in C order:
    /// 1. Hero defender only, once per round: `rnd(-u.uac)` for `AC_VALUE` when
    ///    `u.uac < 0` (`mhitu.c:709`).
    /// 2. Per melee slot `i`: `rnd(20 + i)` (`mhitu.c:806` / `mhitm.c:441`).
    /// 3. On a hit: `n` draws `rnd(d)` for `d(n, d)` (`mhitu.c:1187` /
    ///    `mhitm.c:1025`); none when `n == 0` or `d == 0`.
    /// 4. Hero defender, damage > 0 and `u.uac < 0`: `rnd(-u.uac)` absorption
    ///    (`mhitu.c:1208`).
    ///
    /// The round stops when the defender dies (C: hero death ends the game; `mhitm.c:380` skips attacks once `DEADMONSTER(mdef)`).
    fn resolve_monster_attacks(
        &mut self,
        attacker_id: ActorId,
        defender_id: ActorId,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(attacker) = self.arena.actors.get(attacker_id).cloned() else {
            return events;
        };
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else {
            return events;
        };
        let attacks: &[Attack] = monster_archetype_by_name(&attacker.name)
            .map(|arch| arch.attacks)
            .unwrap_or(std::slice::from_ref(&FALLBACK_ATTACK));
        let def_ac = self.defender_ac(defender_id, &defender);
        let hero_defender = defender_id == self.player_id;
        let m_lev = attacker.level as i32;
        // C mhitu.c:709: tmp = AC_VALUE(u.uac) + 10 + m_lev, computed once.
        let hero_tmp = if hero_defender {
            let ac_roll = if def_ac < 0 {
                self.rng.random_range(1..=def_ac.unsigned_abs())
            } else {
                1
            };
            Some(monster_to_hit_value(m_lev, def_ac, ac_roll))
        } else {
            None
        };

        for (i, attack) in attacks.iter().enumerate() {
            if !is_melee_attack(attack.at) {
                continue;
            }
            let alive = self
                .arena
                .actors
                .get(defender_id)
                .is_some_and(|d| !d.is_dead);
            if !alive {
                break;
            }
            let i = i as u32;
            let (tmp, die) = match hero_tmp {
                Some(tmp) => (tmp, mattacku_die(i)),
                None => mhitm_to_hit(m_lev, def_ac, i),
            };
            let roll = self.rng.random_range(1..=die);
            if !monster_attack_hits(tmp, die, roll) {
                self.push_miss(attacker_id, &attacker, defender_id, &defender, &mut events);
                continue;
            }
            let rolls: Vec<u32> = if attack.d == 0 {
                Vec::new()
            } else {
                (0..attack.n)
                    .map(|_| self.rng.random_range(1..=u32::from(attack.d)))
                    .collect()
            };
            let def_intrinsics = self
                .arena
                .actors
                .get(defender_id)
                .map(|d| d.intrinsics)
                .unwrap_or(defender.intrinsics);
            let is_resisted = resisted(attack.ad, &def_intrinsics);
            let hero = if hero_defender {
                let raw = if is_resisted {
                    0
                } else {
                    monster_attack_damage(attack, &rolls)
                };
                let absorb = if raw > 0 && def_ac < 0 {
                    self.rng.random_range(1..=def_ac.unsigned_abs())
                } else {
                    1
                };
                Some((def_ac, absorb))
            } else {
                None
            };
            let dmg = monster_hit_damage(attack, &rolls, is_resisted, hero);
            if self.land_hit(
                attacker_id,
                &attacker,
                defender_id,
                &defender,
                dmg,
                false,
                &mut events,
            ) {
                break;
            }
        }
        events
    }

    fn push_miss(
        &self,
        attacker_id: ActorId,
        attacker: &ActorRecord,
        defender_id: ActorId,
        defender: &ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        events.push(GameEvent::AttackMissed {
            attacker: attacker_id,
            target: defender_id,
        });
        events.push(GameEvent::LogMessage {
            text: netrust_i18n::Messages::attack_miss(&attacker.name, &defender.name, self.locale),
        });
    }
}
