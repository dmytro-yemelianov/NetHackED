//! Melee combat resolution and damage application.

use nethacked_arena::{ActorId, ActorRecord, ItemLocation};
use nethacked_core::{
    combat::{
        attack_hits, dmgval, is_melee_attack, mattacku_die, mhitm_to_hit, monster_attack_damage,
        monster_attack_hits, monster_hit_damage, monster_to_hit_value, resisted,
        resolve_melee_attack, to_hit_value, weapon_damage_die,
    },
    mines::clamp_luck,
    peace::{adjalign, alignlim, A_NONE},
    religion::clamp_favor,
    Combatant, QuestProgress,
};
use nethacked_data::{monsters::MonsterSound, ItemKindId, MonsterSize};
use nethacked_types::{Attack, AttackType, Buc, DamageType};
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
                        "excalibur" => Some(nethacked_types::ArtifactKind::Excalibur),
                        "vorpal blade" => Some(nethacked_types::ArtifactKind::VorpalBlade),
                        "mjollnir" => Some(nethacked_types::ArtifactKind::Mjollnir),
                        "magicbane" => Some(nethacked_types::ArtifactKind::Magicbane),
                        "the eye of the aethiopica" | "eye of the aethiopica" => {
                            Some(nethacked_types::ArtifactKind::EyeOfTheAethiopica)
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
                    let sk = self
                        .ruleset
                        .item(&w.name)
                        .and_then(|item_def| item_def.weapon_skill());
                    if let Some(skill_class) = sk {
                        let level = self
                            .hero
                            .skills
                            .skills
                            .get(&skill_class)
                            .copied()
                            .unwrap_or(nethacked_types::SkillLevel::Unskilled);
                        skill_hit_bonus = nethacked_core::skills::skill_to_hit_bonus(level);
                        skill_dmg_bonus = nethacked_core::skills::skill_damage_bonus(level);
                    }
                }
            } else {
                let level = self
                    .hero
                    .skills
                    .skills
                    .get(&nethacked_types::SkillClass::BareHanded)
                    .copied()
                    .unwrap_or(nethacked_types::SkillLevel::Unskilled);
                // C weapon.c:1601: bare-handed uses its own table (+1/+2), not -4.
                skill_hit_bonus = nethacked_core::skills::bare_handed_hit_bonus(level);
                skill_dmg_bonus = nethacked_core::skills::skill_damage_bonus(level);
            }
        }

        let weapon_dice = if let Some(wid) = self.wielded_item {
            if let Some(w) = self.arena.items.get(wid) {
                let dice = self
                    .ruleset
                    .item(&w.name)
                    .and_then(|i| i.weapon_damage_dice())
                    .unwrap_or((2, 2));
                Some(dice)
            } else {
                None
            }
        } else {
            None
        };

        let target_large = self
            .ruleset
            .monster(&defender.name)
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
        // C hmon_hitmon (uhitm.c:944 `dmgval`, uhitm.c:847 bare hands) runs
        // only after `tmp > dieroll` (uhitm.c:782): the damage die is drawn
        // only on a hit, so a miss consumes just the rnd(20).
        let base_roll = if attack_hits(d20, to_hit) {
            let die = weapon_damage_die(weapon_dice, target_large, martial_arts);
            let roll = if die == 0 {
                0
            } else {
                self.rng.random_range(1..=die)
            };
            dmgval(weapon_dice, target_large, martial_arts, roll)
        } else {
            0
        };

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
            let is_demon_or_undead = self
                .ruleset
                .monster(&defender.name)
                .map(|m| m.is_demon_or_undead())
                .unwrap_or_else(|| {
                    let lower = defender.name.to_lowercase();
                    lower.contains("demon")
                        || lower.contains("lich")
                        || lower.contains("vampire")
                        || lower.contains("zombie")
                        || lower.contains("skeleton")
                });

            let mut final_damage = result.damage_dealt;
            if let Some(art) = artifact {
                final_damage = nethacked_core::artifacts_wands::resolve_artifact_damage(
                    art,
                    final_damage,
                    is_demon_or_undead,
                );
            }

            let decap = if artifact == Some(nethacked_types::ArtifactKind::VorpalBlade) {
                (self.rng.next_u32() % 20) == 0
            } else {
                false
            };
            if decap {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::vorpal_decapitate(&defender.name, self.locale),
                });
            }
            let lethal = self.land_hit(
                attacker_id,
                &attacker,
                defender_id,
                &defender,
                final_damage,
                decap,
                &mut events,
            );
            // C hmon_hitmon (uhitm.c:1923-1926): a surviving target is woken
            // with `wakeup(mon, TRUE)` -> setmangry.
            if !lethal {
                self.setmangry(defender_id, &mut events);
            }
        } else {
            self.push_miss(attacker_id, &attacker, defender_id, &defender, &mut events);
            // C missum (uhitm.c:5212-5213): `wakeup(mdef, TRUE)` -> setmangry.
            self.setmangry(defender_id, &mut events);
        }

        events
    }

    /// Copy the player actor's `hp`/`max_hp` (the single source of truth for
    /// hero HP) into the hero's current form before damage resolution: the
    /// polyform (`u.mh`/`u.mhmax`) when polymorphed, else the base form
    /// (`u.uhp`/`u.uhpmax`). C keeps one pair per form (`hack.c:4256` losehp).
    pub(crate) fn sync_hero_form_from_actor(hero: &mut nethacked_types::Hero, actor: &ActorRecord) {
        let hp = i32::try_from(actor.hp).unwrap_or(i32::MAX);
        let max_hp = i32::try_from(actor.max_hp).unwrap_or(i32::MAX);
        if let Some(poly) = &mut hero.polymorph {
            poly.hp = hp;
            poly.max_hp = max_hp;
        } else {
            hero.base_hp = hp;
            hero.base_max_hp = max_hp;
        }
    }

    /// Apply `damage` to the hero through C `losehp` (`hack.c:4256`): the
    /// player actor's hp/max_hp are authoritative (healing, breath, traps,
    /// prayer and regeneration change only them), so they are copied into the
    /// current form first, then [`nethacked_core::polymorph::apply_poly_damage`]
    /// runs (`u.mh < 1` -> `rehumanize`, else `u.uhp < 1` -> death) and the
    /// result is written back. Pushes "You revert to your normal form!" on
    /// rehumanize. Returns `true` when the hero died. Shared by melee
    /// (`land_hit`), breath and gaze damage.
    pub(crate) fn damage_hero(&mut self, damage: i32, events: &mut Vec<GameEvent>) -> bool {
        let Some(target) = self.arena.actors.get_mut(self.player_id) else {
            return false;
        };
        Self::sync_hero_form_from_actor(&mut self.hero, target);
        // Unchanging is not tracked on Hero yet; pass false.
        let poly_res = nethacked_core::polymorph::apply_poly_damage(&mut self.hero, damage, false);
        match poly_res {
            nethacked_core::polymorph::PolyDamageResult::Absorbed
            | nethacked_core::polymorph::PolyDamageResult::BaseDamaged => {
                if let Some(poly) = &self.hero.polymorph {
                    target.hp = poly.hp.max(0) as u32;
                    target.max_hp = poly.max_hp.max(0) as u32;
                } else {
                    target.hp = self.hero.base_hp.max(0) as u32;
                    target.max_hp = self.hero.base_max_hp.max(0) as u32;
                }
                false
            }
            nethacked_core::polymorph::PolyDamageResult::Reverted => {
                target.hp = self.hero.base_hp.max(0) as u32;
                target.max_hp = self.hero.base_max_hp.max(0) as u32;
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::revert_form(self.locale).to_string(),
                });
                false
            }
            nethacked_core::polymorph::PolyDamageResult::Dead => {
                target.hp = 0;
                target.is_dead = true;
                true
            }
        }
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
        if defender_id == self.player_id {
            let mut actual_damage = i32::try_from(final_damage).unwrap_or(i32::MAX);
            if decap {
                actual_damage = actual_damage.saturating_add(9999); // Force fatal
            }
            lethal = self.damage_hero(actual_damage, events);
        } else if let Some(target) = self.arena.actors.get_mut(defender_id) {
            let (new_hp, dead) = if decap {
                nethacked_core::artifacts_wands::apply_vorpal_strike(target.hp, decap)
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
        events.push(GameEvent::AttackLanded {
            attacker: attacker_id,
            target: defender_id,
            damage: final_damage,
            lethal,
        });
        let attack_msg = nethacked_i18n::Messages::attack_hit(
            &attacker.name,
            &defender.name,
            final_damage,
            self.locale,
        );
        events.push(GameEvent::LogMessage { text: attack_msg });
        if lethal {
            self.on_actor_killed(attacker_id, defender_id, events);
        }
        lethal
    }

    /// C `iter_mons(anger_quest_guardians)` (`mon.c:3733-3739`): each quest
    /// guardian gets `setmangry(mtmp, TRUE)` (`mon.c:4265-4318`). Only a
    /// peaceful, non-tame guardian turns hostile, costing `adjalign(-1)`.
    /// `setmangry` never calls `set_malign`, so the guardian keeps its peaceful
    /// `malign`. Not modelled: the per-guardian Elbereth hypocrisy check and the
    /// "gets angry!" messages.
    pub fn anger_quest_guardians(&mut self) {
        let quest_cfg = nethacked_core::get_role_quest_config_or_default(&self.role_name);
        let guardian_ids: Vec<ActorId> = self
            .arena
            .actors
            .iter()
            .filter(|(id, actor)| {
                *id != self.player_id
                    && !actor.is_dead
                    && (actor.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
                        || self
                            .ruleset
                            .monster(&actor.name)
                            .is_some_and(|m| m.msound == MonsterSound::Guardian))
            })
            .map(|(id, _)| id)
            .collect();

        let lim = alignlim(self.scheduler.turn);
        for id in guardian_ids {
            let Some(mon) = self.arena.actors.get_mut(id) else {
                continue;
            };
            if !mon.is_peaceful || mon.is_tame {
                continue;
            }
            mon.is_peaceful = false;
            self.alignment_record = adjalign(self.alignment_record, -1, lim);
        }
    }

    /// Centralized actor death resolution (C `mon.c:3676-3726`).
    ///
    /// Handles conduct kill tracking, quest leader / nemesis / guardian kill
    /// penalties and rewards, temple priest divine protection cancellation,
    /// tame / peaceful / hostile alignment adjustments via C `adjalign`,
    /// quest artifact generation, and corpse spawning.
    pub fn on_actor_killed(
        &mut self,
        attacker_id: ActorId,
        defender_id: ActorId,
        events: &mut Vec<GameEvent>,
    ) {
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else {
            return;
        };

        if attacker_id == self.player_id {
            nethacked_core::conducts::record_kill(&mut self.conducts);
        }

        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::killed(&defender.name, self.locale),
        });

        if defender_id != self.player_id {
            let quest_cfg = nethacked_core::get_role_quest_config_or_default(&self.role_name);
            let mdef = self.ruleset.monster(&defender.name);
            // C mon.c:3684 tests `m_id == quest_status.leader_m_id`: only the
            // hero's own quest leader, not any MS_LEADER monster (the Tourist
            // nemesis, Master of Thieves, is MS_LEADER).
            let is_leader = defender.name.eq_ignore_ascii_case(quest_cfg.leader_name);
            let is_nemesis = defender.name.eq_ignore_ascii_case(quest_cfg.nemesis_name)
                || mdef.is_some_and(|m| m.msound == MonsterSound::Nemesis);
            // C mon.c:3691/3694 classify the alignment adjustment by `msound`
            // alone, so the MS_LEADER Tourist nemesis gets no "Real good!" bonus.
            let align_nemesis = mdef.is_some_and(|m| m.msound == MonsterSound::Nemesis);
            let is_guardian = defender.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
                || mdef.is_some_and(|m| m.msound == MonsterSound::Guardian);
            let is_priest = mdef.is_some_and(|m| m.is_priest())
                || defender.name.to_ascii_lowercase().contains("priest");

            if attacker_id == self.player_id {
                let lim = alignlim(self.scheduler.turn);
                let hero_align = self.hero_alignment();

                if is_leader {
                    // REAL BAD! mon.c:3678
                    let penalty = -(self.alignment_record + lim / 2);
                    self.alignment_record = adjalign(self.alignment_record, penalty, lim);
                    self.divine_state.favor = clamp_favor(self.divine_state.favor - 7);
                    self.player_luck = clamp_luck(self.player_luck - 20);
                    self.quest_state.killed_leader = true;
                    let probably = self.quest_state.progress == QuestProgress::Completed;
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::bad_idea(probably, self.locale).into(),
                    });
                    self.anger_quest_guardians();
                } else if align_nemesis {
                    // Real good! mon.c:3691
                    if !self.quest_state.killed_leader {
                        self.alignment_record = adjalign(self.alignment_record, lim / 4, lim);
                    }
                } else if is_guardian {
                    // Bad mon.c:3689
                    self.alignment_record = adjalign(self.alignment_record, -(lim / 8), lim);
                    self.divine_state.favor = clamp_favor(self.divine_state.favor - 1);
                    self.player_luck = clamp_luck(self.player_luck - 4);
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::bad_idea(true, self.locale).into(),
                    });
                } else if is_priest {
                    // C mon.c:3697-3705
                    let coaligned = defender.alignment == hero_align;
                    let adj = if coaligned { -2 } else { 2 };
                    self.alignment_record = adjalign(self.alignment_record, adj, lim);
                    if coaligned {
                        self.divine_protection = 0;
                    }
                    let maligntyp = mdef.map(|m| m.maligntyp).unwrap_or(0);
                    if maligntyp == A_NONE || defender.name.to_ascii_lowercase().contains("moloch")
                    {
                        self.alignment_record = adjalign(self.alignment_record, lim / 4, lim);
                    }
                } else if defender.is_tame {
                    // C mon.c:3706
                    self.alignment_record = adjalign(self.alignment_record, -15, lim);
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::distant_thunder(self.locale).into(),
                    });
                } else if defender.is_peaceful {
                    // C mon.c:3722
                    self.alignment_record = adjalign(self.alignment_record, -5, lim);
                }

                // C mon.c:3725: malign was already adjusted for u.ualign.type and randomization
                self.alignment_record = adjalign(self.alignment_record, defender.malign, lim);
            }

            if is_nemesis {
                nethacked_core::attack_nemesis(&mut self.quest_state, 9999);
                let art_id = match quest_cfg.role_name.to_lowercase().as_str() {
                    "valkyrie" => ItemKindId::ART_ORB_OF_FATE,
                    "wizard" => ItemKindId::ART_EYE_OF_THE_AETHIOPICA,
                    "barbarian" => ItemKindId::ART_HEART_OF_AHRIMAN,
                    "knight" => ItemKindId::ART_MAGIC_MIRROR_OF_MERLIN,
                    "monk" => ItemKindId::ART_EYES_OF_THE_OVERWORLD,
                    "rogue" => ItemKindId::ART_MASTER_KEY_OF_THIEVERY,
                    "tourist" => ItemKindId::ART_YENDORIAN_EXPRESS_CARD,
                    "healer" => ItemKindId::ART_STAFF_OF_AESCULAPIUS,
                    _ => ItemKindId::ART_ORB_OF_DETECTION,
                };
                if let Some(art_rec) = self.ruleset.create_item_record_by_id(
                    art_id,
                    ItemLocation::Floor(defender.coord),
                    Buc::Blessed,
                ) {
                    self.arena.spawn_item(art_rec);
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::quest_nemesis_defeat(
                        quest_cfg.nemesis_name,
                        quest_cfg.artifact_name,
                        self.locale,
                    ),
                });
            }

            if let Some(corpse) = self.ruleset.create_item_record_by_id(
                ItemKindId::CORPSE,
                ItemLocation::Floor(defender.coord),
                Buc::Uncursed,
            ) {
                self.arena.spawn_item(corpse);
            }
        }
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
                .filter(|it| it.class == nethacked_types::ItemClass::Armor)
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
        let rs = std::sync::Arc::clone(&self.ruleset);
        let fallback_slice = [FALLBACK_ATTACK];
        let attacks: &[Attack] = rs
            .monster(&attacker.name)
            .map(|arch| arch.attacks.as_slice())
            .unwrap_or(&fallback_slice);
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
            text: nethacked_i18n::Messages::attack_miss(
                &attacker.name,
                &defender.name,
                self.locale,
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nethacked_types::{Alignment, Coord, Intrinsics};

    /// A hero melee miss draws only the to-hit `rnd(20)` (uhitm.c:780); the
    /// weapon damage roll (`dmgval`, uhitm.c:944) is drawn only on a hit.
    #[test]
    fn hero_miss_does_not_draw_the_damage_roll() {
        let mut sim = SimulationWorld::new_with_seed(77);
        let pid = sim.player_id;
        sim.arena.actors.retain(|id, _| id == pid);
        let at = {
            let p = &sim.arena.actors[pid];
            Coord::new_unchecked(p.coord.x + 1, p.coord.y)
        };
        // AC -60: find_roll_to_hit `tmp` is far below 1, so every rnd(20) misses.
        let dummy = sim.arena.spawn_actor(ActorRecord {
            name: "training dummy".into(),
            coord: at,
            hp: 50,
            max_hp: 50,
            ac: -60,
            level: 0,
            speed: 12,
            alignment: Alignment::Neutral,
            intrinsics: Intrinsics::default(),
            is_player: false,
            is_unique: false,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            abilities: Vec::new(),
            is_peaceful: false,
            mspec_used: 0,
            malign: 0,
        });
        for _ in 0..20 {
            let mut expected = sim.rng.clone();
            let _d20: u32 = expected.random_range(1..=20u32);
            let events = sim.resolve_combat(pid, dummy);
            assert!(events
                .iter()
                .any(|e| matches!(e, GameEvent::AttackMissed { .. })));
            assert_eq!(sim.rng, expected, "a miss draws only the rnd(20)");
        }
    }
}
