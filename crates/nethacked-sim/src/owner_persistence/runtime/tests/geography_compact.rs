//! Acceptance tests for the compact canonical geography codec
//! (`nethacked_dungeon::compact_level`) that resolved the M7 review
//! condition C1: the 16 KiB transports carry the full `DungeonLevel` in
//! `GeographyFacts`, whose natural JSON reached 15.4-17.4 KiB on fresh
//! levels (measured over seeds 1-200), so over-bound worlds refused every
//! geography-reading action with the state untouched. The codec encodes
//! the tile grid as one byte per cell inside a hex string, with a small
//! side table for `BranchStairs` levels.
//!
//! These tests pin the three properties the owner pipeline relies on:
//! losslessness (the decoded level is identical to the original in its
//! natural encoding), canonicality (re-encoding reproduces the exact bytes,
//! which the owner `decode` path requires) and the payload bound (every
//! fresh level fits the 16 KiB kernel limit with a wide margin), plus an
//! end-to-end facade dispatch on a seed that was over-bound under the
//! natural encoding.

use super::super::*;
use super::m7_audit;
use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_dungeon::DungeonLevel;
use nethacked_types::{
    Alignment, BranchId, Coord, DoorState, Tile, TrapRecord, TrapState, TrapType,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Read access to the geography core every transport's `GeographyFacts`
/// carries, so one helper can check all of them.
trait GeographyView {
    fn level(&self) -> &DungeonLevel;
    fn branch(&self) -> BranchId;
    fn depth(&self) -> usize;
}

impl GeographyView for action_time::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

impl GeographyView for movement_actions::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

impl GeographyView for inventory_actions::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

impl GeographyView for item_effects::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

impl GeographyView for services::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

impl GeographyView for monster_ticks::GeographyFacts {
    fn level(&self) -> &DungeonLevel {
        &self.level
    }
    fn branch(&self) -> BranchId {
        self.branch
    }
    fn depth(&self) -> usize {
        self.depth
    }
}

/// Round-trip one facts value through the compact wire encoding and require
/// losslessness (natural-encoding equality of the level), whole-struct
/// equality and byte-identical re-encoding (canonicality).
fn assert_compact_roundtrip<T>(facts: &T)
where
    T: Serialize + DeserializeOwned + GeographyView,
{
    let bytes = serde_json::to_vec(facts).unwrap();
    let back: T = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(back.level()).unwrap(),
        serde_json::to_value(facts.level()).unwrap(),
        "compact level roundtrip lost level information"
    );
    assert_eq!(back.branch(), facts.branch());
    assert_eq!(back.depth(), facts.depth());
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(facts).unwrap(),
        "compact facts roundtrip lost non-level fields"
    );
    assert_eq!(
        serde_json::to_vec(&back).unwrap(),
        bytes,
        "compact encoding is not canonical (owner decode requires exact re-encode)"
    );
}

#[test]
fn compact_geography_roundtrip_is_lossless_and_canonical_across_seeds() {
    for seed in 1..=200u64 {
        let w = SimulationWorld::new_with_seed(seed);
        let geo = action_time::GeographyFacts {
            level: w.level.clone(),
            branch: w.current_branch,
            depth: w.depth,
        };
        assert_compact_roundtrip(&geo);
    }
}

#[test]
fn compact_geography_roundtrip_covers_every_transport_and_rich_level() {
    let mut w = SimulationWorld::new_with_seed(4);
    // Exercise every tile payload nibble and the branch-stairs side table.
    let rich: [(usize, usize, Tile); 12] = [
        (
            2,
            2,
            Tile::BranchStairs {
                branch: BranchId::Sokoban,
                level: 3,
                up: false,
            },
        ),
        (
            3,
            2,
            Tile::BranchStairs {
                branch: BranchId::RogueLevel,
                level: 1,
                up: true,
            },
        ),
        (
            4,
            2,
            Tile::Door {
                state: DoorState::Locked,
                trapped: true,
            },
        ),
        (5, 2, Tile::SecretDoor { locked: true }),
        (6, 2, Tile::Pit { filled: false }),
        (
            7,
            2,
            Tile::Altar {
                align: Alignment::Chaotic,
            },
        ),
        (
            8,
            2,
            Tile::HighAltar {
                align: Alignment::Lawful,
            },
        ),
        (9, 2, Tile::Drawbridge { open: true }),
        (10, 2, Tile::Pool { frozen: true }),
        (11, 2, Tile::Wall { horizontal: true }),
        (12, 2, Tile::Lava),
        (13, 2, Tile::Corr),
    ];
    for (x, y, tile) in rich {
        w.level.set_tile(Coord::new_unchecked(x, y), tile);
    }
    // Sorted engravings and traps with non-trivial payloads.
    w.level.engravings.insert(
        Coord::new_unchecked(2, 3),
        Engraving::new("Elbereth", EngravingMedium::Burned),
    );
    w.level.engravings.insert(
        Coord::new_unchecked(5, 3),
        Engraving::new("x", EngravingMedium::Carved(7)),
    );
    let trap_coord = Coord::new_unchecked(3, 3);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 1,
            trap_type: TrapType::Pit,
            state: TrapState::Revealed,
            coord: trap_coord,
        },
    );

    let level = w.level.clone();
    let branch = w.current_branch;
    let depth = w.depth;
    assert_compact_roundtrip(&action_time::GeographyFacts {
        level: level.clone(),
        branch,
        depth,
    });
    assert_compact_roundtrip(&movement_actions::GeographyFacts {
        level: level.clone(),
        branch,
        depth,
    });
    assert_compact_roundtrip(&inventory_actions::GeographyFacts {
        level: level.clone(),
        branch,
        depth,
    });
    assert_compact_roundtrip(&monster_ticks::GeographyFacts {
        level: level.clone(),
        branch,
        depth,
    });
    assert_compact_roundtrip(&item_effects::GeographyFacts {
        level: level.clone(),
        branch,
        depth,
        vibrating_square: Some(Coord::new_unchecked(1, 1)),
        stored_keys: vec![(BranchId::GnomishMines, 2), (BranchId::RogueLevel, 1)],
    });
    assert_compact_roundtrip(&services::GeographyFacts {
        level,
        branch,
        depth,
        vibrating_square: None,
        stored_keys: vec![],
    });
}

#[test]
fn compact_geography_fits_kernel_bound_across_seeds() {
    let mut max_compact = 0;
    let mut max_natural = 0;
    for seed in 1..=200u64 {
        let w = SimulationWorld::new_with_seed(seed);
        let geo = action_time::GeographyFacts {
            level: w.level.clone(),
            branch: w.current_branch,
            depth: w.depth,
        };
        let compact = serde_json::to_vec(&geo).unwrap().len();
        let natural = serde_json::to_vec(&w.level).unwrap().len();
        max_compact = max_compact.max(compact);
        max_natural = max_natural.max(natural);
        assert!(
            compact <= 12 * 1024,
            "seed {seed} compact geography facts exceed the 12 KiB margin target (got {compact})"
        );
        if matches!(seed, 1 | 4 | 9) {
            // These seeds serialized to 17027, 17572 and 17041 bytes as full
            // natural facts: over the 16 KiB bound, so they refused before.
            assert!(
                natural > m7_audit::KERNEL_PAYLOAD,
                "seed {seed} natural level encoding must exceed the kernel bound (got {natural})"
            );
        }
    }
    // The natural encoding really did exceed the 16 KiB bound on fresh
    // levels, which is the refusal this codec resolves.
    assert!(
        max_natural > m7_audit::KERNEL_PAYLOAD,
        "natural level encoding no longer exceeds the kernel bound (max {max_natural})"
    );
    assert!(
        max_compact * 3 <= m7_audit::KERNEL_PAYLOAD,
        "compact geography facts lost their margin (max {max_compact})"
    );
}

#[test]
fn facade_dispatches_combined_script_on_previously_over_bound_seed() {
    // Seed 4's fresh level serialized to 17572 bytes under the natural
    // encoding — over the 16 KiB kernel bound — so every geography-reading
    // action refused. With the compact codec the full 24-step combined
    // script (every migrated transport) dispatches bit-for-bit against the
    // legacy oracle.
    let w = m7_audit::m7_world(4);
    let hc = w.arena.actors[w.player_id].coord;
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;
    for ast in m7_audit::m7_script(hc) {
        m7_audit::facade_oracle_step(&mut session, &mut legacy, ast);
    }
    // The traversal phase really ran: descend packed the fresh level and
    // ascend restored it, leaving one stored level behind.
    assert_eq!(session.world().depth, 1);
    assert_eq!(session.world().stored_levels.len(), 1);
}
