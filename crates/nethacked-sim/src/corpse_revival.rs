//! Selected ordinary Troll family, source-only C 16ff5911: zap.c:713–1122,
//! do.c:2251–2294, teleport.c:53–275,578–712, makemon.c:1179–1299.
//! Global IDs/wrap, mextra/pet abuse, apparent hero position, unique
//! substitution, burial/contents and Rider/Ice families remain separate owners.
use crate::{ObjectTimerKind, ObjectTimerTarget, SimulationWorld};
use nethacked_arena::ItemId;
use rand::Rng;

impl SimulationWorld {
    /// Expiry was consumed and reported before this callback. Unsupported
    /// families remain neutral; ordinary failures reschedule fresh five-d50 rot.
    pub(crate) fn revive_ordinary_troll(&mut self, id: ItemId) {
        let input = crate::revival_law::RevivalInput {
            ruleset: &self.ruleset,
            items: &self.arena.items,
            actors: &self.arena.actors,
            level: &self.level,
            genocide_registry: &self.genocide_registry,
            player: self.player_id,
            branch: self.current_branch,
            depth: self.depth,
            hero_alignment: self.hero_alignment(),
            hero_race: self.hero_race,
            alignment_record: self.alignment_record,
            role_name: &self.role_name,
        };
        let attempt = crate::revival_law::ordinary_troll_attempt(id, &input, &mut self.rng);
        let target = ObjectTimerTarget::Active(id);
        match attempt {
            crate::revival_law::RevivalAttempt::Unsupported => {}
            crate::revival_law::RevivalAttempt::Failed { rot_delay } => {
                if !self
                    .object_timers
                    .entries()
                    .iter()
                    .any(|t| t.target == target && t.kind == ObjectTimerKind::RotCorpse)
                {
                    self.start_object_timer(target, ObjectTimerKind::RotCorpse, rot_delay)
                        .expect("revival rot deadline fits");
                }
            }
            crate::revival_law::RevivalAttempt::Revived { actor } => {
                self.normalize_revival_actor_allocator();
                self.arena.spawn_actor(actor);
                if self.arena.items[id].quantity > 1 {
                    let _split_identity = self.rng.random_range(0..2u32);
                    self.consume_split_timer_serials(target);
                    self.arena.items[id].quantity -= 1;
                } else {
                    self.cancel_object_timers(target);
                    self.arena.destroy_item(id);
                    self.forget_rot_corpse_references(id);
                }
            }
        }
    }
}
