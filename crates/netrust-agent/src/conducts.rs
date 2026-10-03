use netrust_sim::ActionAst;
use netrust_types::ConductTracker;

/// Helper to filter out actions that would violate the currently active conducts.
pub fn filter_conduct_violations(
    actions: Vec<ActionAst>,
    conducts: &ConductTracker,
) -> Vec<ActionAst> {
    actions
        .into_iter()
        .filter(|a| {
            match a {
                ActionAst::MeleeAttack(_) | ActionAst::Cast { .. } => {
                    if conducts.pacifist {
                        return false;
                    }
                }
                ActionAst::Eat(..) => {
                    // For a true vegan/vegetarian policy, we would need to inspect the item to see if it's meat.
                    // For now, if either is active, they might want to avoid all unknown eating or just meat.
                    // But we can't fully know without item details. We'll leave Eat here but AI should verify.
                }
                ActionAst::Read(..) => {
                    if conducts.illiterate {
                        return false;
                    }
                }
                ActionAst::Pray | ActionAst::Sacrifice(_) => {
                    if conducts.atheist {
                        return false;
                    }
                }
                ActionAst::Wish(_) => {
                    if conducts.wishless {
                        return false;
                    }
                }
                ActionAst::ZapWand { .. } => {
                    // if it's polymorph, they might violate polypileless. But we can't know target easily here.
                }
                _ => {}
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrust_sim::ActionAst;
    use netrust_types::ConductTracker;
    use netrust_sim::Coord;
    use netrust_sim::Direction;

    #[test]
    fn test_agent_conduct_filter_masks_attacks() {
        let actions = vec![
            ActionAst::Wait,
            ActionAst::MeleeAttack(Coord::new(0, 0).unwrap()),
            ActionAst::Move(Direction::East),
        ];
        
        let mut conducts = ConductTracker::default();
        conducts.pacifist = true;
        
        let filtered = filter_conduct_violations(actions.clone(), &conducts);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&ActionAst::Wait));
        assert!(filtered.contains(&ActionAst::Move(Direction::East)));
        
        conducts.pacifist = false;
        let filtered2 = filter_conduct_violations(actions, &conducts);
        assert_eq!(filtered2.len(), 3);
    }
}
