use nethacked_types::ItemClass;

#[derive(Debug, Clone)]
pub struct Item {
    pub name: String,
    pub class: ItemClass,
}

pub fn polypile_stack(items: &[Item], rng_seed: u64) -> Vec<Item> {
    let mut rng = rng_seed;
    let mut next_rand = || -> u64 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };

    let mut result = Vec::new();

    for item in items {
        let val = next_rand() % 100;
        if val < 20 {
            // 20% system shock
            if val < 10 {
                let mut new_item = item.clone();
                new_item.name = "rock".to_string();
                new_item.class = ItemClass::Rock;
                result.push(new_item);
            }
        } else {
            let mut new_item = item.clone();
            new_item.name = "polymorphed item".to_string();
            result.push(new_item);
        }
    }
    result
}
