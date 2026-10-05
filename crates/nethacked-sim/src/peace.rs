//! Monster peacefulness in the sim: C `peace_minded` at creation
//! (`makemon.c:1299`), `setmangry` when the hero attacks (`mon.c:4265-4318`)
//! and the Elbereth `onscary` test (`monmove.c:240-302`).

use nethacked_arena::{ActorId, ActorRecord};
use nethacked_core::engraving::Engraving;
use nethacked_core::{adjalign, alignlim, calculate_malign, peace_minded, PeaceMindedInput};
use nethacked_data::ruleset::{MonsterDef, Ruleset};
use nethacked_data::{race_hostile, race_peaceful, MonsterSound, MonsterSpeciesId, RaceId};
use nethacked_types::{Alignment, Coord};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// Whether C `Monnam` (`x_monnam`, `do_name.c`) puts "the" before this monster:
/// every non-unique monster (unique ones carry proper names).
pub(crate) fn monnam_article(ruleset: &Ruleset, name: &str) -> bool {
    !ruleset.monster(name).is_some_and(|a| a.is_unique)
}

/// C `u.ualign.type` as a number (`A_LAWFUL` 1, `A_NEUTRAL` 0, `A_CHAOTIC` -1).
pub(crate) fn alignment_type(a: Alignment) -> i32 {
    match a {
        Alignment::Lawful => 1,
        Alignment::Neutral => 0,
        Alignment::Chaotic | Alignment::Unaligned => -1,
    }
}

/// The `peace_minded` inputs for `arch` and the given hero state.
pub(crate) fn peace_input(
    arch: &MonsterDef,
    hero_alignment: Alignment,
    hero_race: RaceId,
    hero_align_record: i32,
    hero_has_amulet: bool,
) -> PeaceMindedInput {
    PeaceMindedInput {
        always_peaceful: arch.peaceful_by_default,
        always_hostile: arch.always_hostile,
        leader_or_guardian: matches!(arch.msound, MonsterSound::Leader | MonsterSound::Guardian),
        nemesis: arch.msound == MonsterSound::Nemesis,
        race_peaceful: race_peaceful(hero_race, arch.m2_race),
        race_hostile: race_hostile(hero_race, arch.m2_race),
        monster_alignment: i32::from(arch.maligntyp),
        hero_alignment: alignment_type(hero_alignment),
        hero_align_record,
        hero_has_amulet,
        is_minion: false,
    }
}

/// C `peace_minded` with the C `rn2` draws taken from `rng`
/// (`rn2(n)` = `random_range(0..n)`).
pub(crate) fn roll_peace_minded(input: &PeaceMindedInput, rng: &mut impl Rng) -> bool {
    peace_minded(input, |n| rng.random_range(0..n))
}

/// Whether the monster is a temple priest (C `ispriest`): the sim creates the
/// aligned cleric only as a temple priest (Minetown, Moloch's Sanctum).
fn is_temple_priest(ruleset: &Ruleset, mon: &ActorRecord) -> bool {
    ruleset
        .monster(&mon.name)
        .is_some_and(|a| a.id == Some(MonsterSpeciesId::ALIGNED_CLERIC))
}

/// C `isshk`: the sim's shopkeepers are the `shopkeeper` species.
fn is_shopkeeper(ruleset: &Ruleset, mon: &ActorRecord) -> bool {
    ruleset
        .monster(&mon.name)
        .is_some_and(|a| a.id == Some(MonsterSpeciesId::SHOPKEEPER))
}

impl SimulationWorld {
    /// C `u.uhave.amulet`: the hero carries the real Amulet of Yendor.
    pub(crate) fn hero_has_amulet(&self) -> bool {
        self.arena
            .items_carried_by(self.player_id)
            .into_iter()
            .any(|iid| {
                self.arena
                    .items
                    .get(iid)
                    .is_some_and(crate::actions::items::is_real_amulet)
            })
    }

    pub(crate) fn hero_alignment(&self) -> Alignment {
        self.arena
            .actors
            .get(self.player_id)
            .map(|p| p.alignment)
            .unwrap_or(Alignment::Neutral)
    }

    /// C `makemon` (`makemon.c:1299`): `mpeaceful = peace_minded(ptr)` for a
    /// newly created monster of `species`, drawing from the sim RNG only when C does.
    pub(crate) fn roll_spawn_peaceful(&mut self, species: MonsterSpeciesId) -> bool {
        let rs = std::sync::Arc::clone(&self.ruleset);
        let Some(def) = rs.monster_by_id(species) else {
            return false;
        };
        let input = peace_input(
            def,
            self.hero_alignment(),
            self.hero_race,
            self.alignment_record,
            self.hero_has_amulet(),
        );
        roll_peace_minded(&input, &mut self.rng)
    }

    /// C `set_malign(mtmp)` (`makemon.c:2320-2366`): precalculate alignment adjustment upon monster death.
    pub(crate) fn set_monster_malign(&self, rec: &mut ActorRecord, def: &MonsterDef) {
        let quest_cfg = nethacked_core::get_role_quest_config_or_default(&self.role_name);
        let is_leader = matches!(def.msound, MonsterSound::Leader)
            || rec.name.eq_ignore_ascii_case(quest_cfg.leader_name);
        rec.malign = calculate_malign(
            def.maligntyp,
            self.hero_alignment(),
            rec.is_peaceful,
            is_leader,
            def.peaceful_by_default,
            def.always_hostile,
        );
    }

    /// C `onscary(x, y, mtmp)` (`monmove.c:240-302`) for the engraving under the
    /// hero: does a written Elbereth scare `mon`?
    ///
    /// `@`-class monsters (`mlet == S_HUMAN`, monmove.c:260) are taken from the
    /// archetype glyph. No BESTIARY entry is a minotaur, a Rider or a vault guard,
    /// so those exemptions are always false here (the core predicate keeps them).
    pub(crate) fn elbereth_scares(&self, mon: &ActorRecord, engraving: Option<&Engraving>) -> bool {
        let arch = self.ruleset.monster(&mon.name);
        let is_s_human = arch.is_some_and(|a| a.is_human);
        let exempt = nethacked_core::engraving::onscary_exempt(
            is_s_human,
            false,
            is_shopkeeper(&self.ruleset, mon),
            false,
        );
        nethacked_core::engraving::is_elbereth_ward_active(
            engraving,
            mon.intrinsics.blind,
            mon.is_unique,
            mon.is_peaceful,
            exempt,
        )
    }

    /// C `setmangry(mtmp, TRUE)` (`mon.c:4265-4318`), called after the hero's
    /// attack on a surviving monster (`wakeup(mon, TRUE)`, uhitm.c:1926, :5213).
    ///
    /// - Attacking from an Elbereth square a monster Elbereth scares (or a
    ///   peaceful one) is hypocritical: `adjalign(record > 5 ? -5 : -rnd(5))`
    ///   and the engraving is erased (mon.c:4267-4285).
    /// - A peaceful, non-tame target turns hostile; temple priest: `-5` if
    ///   co-aligned (`p_coaligned`, priest.c:370), else `+2`; anyone else `-1`
    ///   (mon.c:4296-4303), then "<Mon> gets angry!" (mon.c:4304-4306).
    /// - Attacking the hero's own quest leader angers the quest guardians
    ///   (`qst_guardians_respond`, mon.c:4312-4313, :4135-4159). C counts only
    ///   guardians the hero can see for the message; the sim counts all of them.
    ///
    /// Not modelled: `growl()` for non-humanoids (every peaceful the sim can make
    /// is humanoid), `peacefuls_respond` (mon.c:4315-4317), `u.ualign.abuse`.
    pub(crate) fn setmangry(&mut self, target_id: ActorId, events: &mut Vec<GameEvent>) {
        let Some(target) = self.arena.actors.get(target_id).cloned() else {
            return;
        };
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return;
        };
        let lim = alignlim(self.scheduler.turn);
        let hero_square: Coord = player.coord;
        let engraving = self
            .level
            .get_engraving(hero_square)
            .filter(|e| e.text == "Elbereth")
            .cloned();
        if let Some(e) = engraving.as_ref() {
            if self.elbereth_scares(&target, Some(e)) || target.is_peaceful {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::feel_hypocrite(self.locale).into(),
                });
                // mon.c:4280 `adjalign((u.ualign.record > 5) ? -5 : -rnd(5))`.
                let n = if self.alignment_record > 5 {
                    -5
                } else {
                    -(self.rng.random_range(1..=5i32))
                };
                self.alignment_record = adjalign(self.alignment_record, n, lim);
                if !player.intrinsics.blind {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::engraving_fades(self.locale).into(),
                    });
                }
                self.level.remove_engraving(hero_square);
            }
        }

        if !target.is_peaceful || target.is_tame {
            return;
        }
        if let Some(t) = self.arena.actors.get_mut(target_id) {
            t.is_peaceful = false;
        }
        let delta = if is_temple_priest(&self.ruleset, &target) {
            // priest.c:370 p_coaligned: u.ualign.type == priest alignment.
            if target.alignment == player.alignment {
                -5
            } else {
                2
            }
        } else {
            -1
        };
        self.alignment_record = adjalign(self.alignment_record, delta, lim);
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::gets_angry(
                &target.name,
                monnam_article(&self.ruleset, &target.name),
                self.locale,
            ),
        });

        let quest_cfg = nethacked_core::get_role_quest_config_or_default(&self.role_name);
        if target.name.eq_ignore_ascii_case(quest_cfg.leader_name) {
            let mut got_mad = 0u32;
            for (_, mon) in self.arena.actors.iter_mut() {
                if !mon.is_dead
                    && mon.is_peaceful
                    && mon.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
                {
                    mon.is_peaceful = false;
                    got_mad += 1;
                }
            }
            if got_mad > 0 {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::guardians_angry_too(
                        quest_cfg.guardian_name,
                        got_mad > 1,
                        self.locale,
                    ),
                });
            }
        }
    }
}
