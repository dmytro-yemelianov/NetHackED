//! Represented ordinary-container contents (mkobj.c:304-393).
//! Species, food variants, fuel, erosion, ice boxes and artifacts remain unsupported.
use crate::item_init::{ContainerInit, ItemInitProfile};
use crate::items::BOXIPROBS;
use crate::{ItemDef, ItemKindId, Ruleset};
use nethacked_arena::{EntityArena, ItemId, ItemLocation, ItemRecord};
use nethacked_types::ItemClass;

/// Context read by mkbox_cnts; callers supply their actual generation context.
#[derive(Debug, Clone, Copy)]
pub struct ContainerContext {
    pub hero_level: u32,
    pub level_difficulty: u32,
    pub moves: u64,
    pub in_mklev: bool,
}

/// Base/stack weights stay on each record; the arena computes contained weight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedItem {
    pub item: ItemRecord,
    pub contents: Vec<GeneratedItem>,
}
impl GeneratedItem {
    /// Attach in generation order, with no additional random draws.
    pub fn attach(mut self, arena: &mut EntityArena, location: ItemLocation) -> ItemId {
        self.item.location = location;
        let id = arena.spawn_item(self.item);
        for child in self.contents {
            child.attach(arena, ItemLocation::InContainer(id));
        }
        id
    }
}

fn draw(rn2: &mut impl FnMut(u32) -> u32, n: u32) -> u32 {
    rn2(n).min(n - 1)
}

impl Ruleset {
    /// Owned definitions in C id order, then custom definitions in pack order.
    /// Checked accumulation also makes direct, unvalidated Rulesets safe.
    fn weighted_item(
        &self,
        eligible: impl Fn(&ItemDef) -> bool,
        rn2: &mut impl FnMut(u32) -> u32,
    ) -> Option<&ItemDef> {
        let mut defs: Vec<_> = self.items.iter().filter(|d| eligible(d)).collect();
        defs.sort_by_key(|d| d.id.map(|id| id.index()).unwrap_or(usize::MAX));
        let total = defs
            .iter()
            .try_fold(0u32, |sum, d| sum.checked_add(d.generation_weight))?;
        if total == 0 {
            return None;
        }
        let mut value = draw(rn2, total);
        for def in defs {
            if value < def.generation_weight {
                return Some(def);
            }
            value -= def.generation_weight;
        }
        None
    }

    /// Box's class draw followed by owned oc_prob kind selection (mkobj.c:349).
    pub fn box_item_definition(&self, mut rn2: impl FnMut(u32) -> u32) -> Option<&ItemDef> {
        let mut value = draw(&mut rn2, 100);
        let mut class = BOXIPROBS.last()?.1;
        for &(weight, candidate) in BOXIPROBS {
            if value < weight {
                class = candidate;
                break;
            }
            value -= weight;
        }
        self.weighted_item(|d| d.class == class, &mut rn2)
    }

    /// C objnam.c:5403 rnd_class, including equal-weight fallback for zero sum.
    /// Ranges intentionally use canonical ids, never display names or custom ids.
    pub fn ranged_item_definition(
        &self,
        first: ItemKindId,
        last: ItemKindId,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<&ItemDef> {
        if first > last {
            return None;
        }
        if first == last {
            return self.item_by_id(first);
        }
        let eligible = |d: &ItemDef| d.id.is_some_and(|id| id >= first && id <= last);
        let total = self
            .items
            .iter()
            .filter(|d| eligible(d))
            .try_fold(0u32, |sum, d| sum.checked_add(d.generation_weight))?;
        if total != 0 {
            return self.weighted_item(eligible, &mut rn2);
        }
        let mut defs: Vec<_> = self.items.iter().filter(|d| eligible(d)).collect();
        defs.sort_by_key(|d| d.id);
        if defs.is_empty() {
            return None;
        }
        Some(defs[draw(&mut rn2, defs.len() as u32) as usize])
    }

    /// Fresh represented object plus recursively initialized contents. Artifact
    /// hooks are disabled throughout. Unlike the fields-only API this rolls counts.
    /// At nesting depth 8 or 1024 records, containers retain lock/trap state but
    /// remain empty without a count draw. This bounds malformed custom rulesets.
    pub fn create_random_item_tree(
        &self,
        name: &str,
        location: ItemLocation,
        ctx: ContainerContext,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<GeneratedItem> {
        let mut budget = 1024;
        self.item_tree(name, location, ctx, 0, &mut budget, &mut rn2)
    }

    fn item_tree(
        &self,
        name: &str,
        location: ItemLocation,
        ctx: ContainerContext,
        depth: u32,
        budget: &mut u32,
        rn2: &mut impl FnMut(u32) -> u32,
    ) -> Option<GeneratedItem> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let def = self.item(name)?;
        let item = self.create_random_item_supported_fields_by_name(
            name,
            location,
            ctx.hero_level,
            &mut *rn2,
        )?;
        let mut tree = GeneratedItem {
            item,
            contents: Vec::new(),
        };
        let ItemInitProfile::Container { container } = def.initialization else {
            return Some(tree);
        };
        if !def.is_container
            || def.is_bag_of_holding != (container == ContainerInit::HoldingBag)
            || !def.initialization.matches_class(def.class)
            || depth >= 8
            || *budget == 0
        {
            return Some(tree);
        }
        let max = match container {
            ContainerInit::Chest => {
                if tree.item.locked {
                    7
                } else {
                    5
                }
            }
            ContainerInit::LargeBox => {
                if tree.item.locked {
                    5
                } else {
                    3
                }
            }
            ContainerInit::Sack | ContainerInit::OilskinSack if ctx.moves <= 1 && !ctx.in_mklev => {
                0
            }
            _ => 1,
        };
        let count = draw(rn2, max + 1);
        for _ in 0..count {
            if *budget == 0 {
                break;
            }
            let Some(child_def) = self.box_item_definition(&mut *rn2) else {
                continue;
            };
            let Some(mut child) = self.item_tree(
                &child_def.name,
                ItemLocation::Limbo,
                ctx,
                depth + 1,
                budget,
                rn2,
            ) else {
                continue;
            };
            if child.item.class == ItemClass::Coin {
                child.item.quantity = (draw(rn2, ctx.level_difficulty.saturating_add(2).max(1))
                    + 1)
                .saturating_mul(draw(rn2, 75) + 1);
                // mkobj.c:1966: coin weight is rounded per hundred, not per coin.
                child.item.weight = (child.item.quantity.saturating_add(50) / 100).max(1);
            } else if child_def.id == Some(ItemKindId::ROCK) {
                if let Some(replacement) = self.ranged_item_definition(
                    ItemKindId::DILITHIUM_CRYSTAL,
                    ItemKindId::LOADSTONE,
                    &mut *rn2,
                ) {
                    if child.item.quantity > 2 {
                        child.item.quantity = 1;
                    }
                    replace_kind(&mut child.item, replacement);
                }
            }
            if container == ContainerInit::HoldingBag {
                if child_def.id.is_some_and(|id| {
                    id == ItemKindId::BAG_OF_HOLDING || id == ItemKindId::BAG_OF_TRICKS
                }) || child.item.is_bag_of_holding
                {
                    if let Some(sack) = self.item_by_id(ItemKindId::SACK) {
                        replace_kind(&mut child.item, sack);
                        child.item.enchantment = 0;
                    }
                } else if child_def.id == Some(ItemKindId::WAN_CANCELLATION) {
                    // C retries over the full range (which includes cancellation).
                    // Cap pathological pack weights at 64 attempts, then choose
                    // the first surviving canonical safe kind without a draw.
                    let mut replacement = None;
                    for _ in 0..64 {
                        let Some(wand) = self.ranged_item_definition(
                            ItemKindId::WAN_LIGHT,
                            ItemKindId::WAN_LIGHTNING,
                            &mut *rn2,
                        ) else {
                            break;
                        };
                        if wand.id != Some(ItemKindId::WAN_CANCELLATION) {
                            replacement = Some(wand);
                            break;
                        }
                    }
                    let replacement = replacement.or_else(|| {
                        self.items
                            .iter()
                            .filter(|d| {
                                d.id.is_some_and(|id| {
                                    id >= ItemKindId::WAN_LIGHT
                                        && id <= ItemKindId::WAN_LIGHTNING
                                        && id != ItemKindId::WAN_CANCELLATION
                                })
                            })
                            .min_by_key(|d| d.id)
                    });
                    let Some(wand) = replacement else {
                        continue;
                    };
                    replace_kind(&mut child.item, wand);
                }
            }
            tree.contents.push(child);
        }
        Some(tree)
    }
}

fn replace_kind(item: &mut ItemRecord, def: &ItemDef) {
    item.name.clone_from(&def.name);
    item.class = def.class;
    item.weight = def.weight.saturating_mul(item.quantity);
    item.is_container = def.is_container;
    item.is_bag_of_holding = def.is_bag_of_holding;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursion_limits_keep_initialized_parent_without_count_draw() {
        let rules = Ruleset::vanilla();
        let ctx = ContainerContext {
            hero_level: 1,
            level_difficulty: 1,
            moves: 2,
            in_mklev: true,
        };
        for (depth, mut budget) in [(8, 100), (0, 1)] {
            let tree = rules
                .item_tree(
                    &rules.item_by_id(ItemKindId::SACK).unwrap().name,
                    ItemLocation::Limbo,
                    ctx,
                    depth,
                    &mut budget,
                    &mut |_| panic!("bounded contents must not draw"),
                )
                .unwrap();
            assert!(tree.contents.is_empty());
        }
    }
}
