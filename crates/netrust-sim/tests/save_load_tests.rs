//! Save/load fidelity: full world JSON round-trip including RNG stream and coord-keyed maps.

use netrust_core::engraving::{Engraving, EngravingMedium};
use netrust_sim::{Coord, SimulationWorld};
use netrust_types::{TrapRecord, TrapState, TrapType};
use rand::RngCore;

#[test]
fn world_roundtrip_preserves_rng_traps_and_engravings() {
    let mut sim = SimulationWorld::new_with_seed(4242);
    let c1 = Coord::new(10, 5).unwrap();
    let c2 = Coord::new(11, 5).unwrap();
    sim.level
        .set_engraving(c1, Engraving::new("Elbereth", EngravingMedium::Dust(5)));
    sim.level.traps.insert(
        c2,
        TrapRecord {
            id: 1,
            trap_type: TrapType::Arrow,
            state: TrapState::Hidden,
            coord: c2,
        },
    );
    // Advance RNG away from its seed position.
    for _ in 0..7 {
        sim.rng.next_u32();
    }

    let json = serde_json::to_string(&sim).expect("serialize");
    let mut restored: SimulationWorld = serde_json::from_str(&json).expect("deserialize");

    // Compare as Value so HashMap key order in objects does not matter.
    let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
    let v2: serde_json::Value = serde_json::to_value(&restored).unwrap();
    assert_eq!(v1, v2);

    assert_eq!(
        restored.level.get_engraving(c1).map(|e| e.text.clone()),
        Some("Elbereth".to_string())
    );
    assert_eq!(
        restored.level.traps.get(&c2).map(|t| t.trap_type),
        Some(TrapType::Arrow)
    );
    assert_eq!(sim.rng.next_u64(), restored.rng.next_u64());
}
