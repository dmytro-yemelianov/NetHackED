//! Test-only adapter for source-derived, prescribed bounded core draws.
use rand::RngCore;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawBlock {
    pub bound: u32,
    pub values: Vec<u32>,
}

/// Maps each prescribed zero-based result to the midpoint of its rand 0.9
/// u32 multiply-high bucket. This is a Rust test adapter, not C RNG emulation.
/// Restricting bounds to 4/8 avoids rejection draws and pins this convention.
pub struct PrescribedRng {
    words: std::vec::IntoIter<u32>,
}

impl PrescribedRng {
    pub fn new(blocks: &[DrawBlock]) -> Self {
        let words = blocks
            .iter()
            .flat_map(|block| {
                assert!(matches!(block.bound, 4 | 8), "unsupported draw bound");
                let width = (1u64 << 32) / u64::from(block.bound);
                block.values.iter().map(move |&value| {
                    assert!(value < block.bound, "draw result outside bound");
                    (u64::from(value) * width + width / 2) as u32
                })
            })
            .collect::<Vec<_>>()
            .into_iter();
        Self { words }
    }

    pub fn assert_exhausted(&self) {
        assert_eq!(self.words.len(), 0, "production skipped prescribed draws");
    }
}

impl RngCore for PrescribedRng {
    fn next_u32(&mut self) -> u32 {
        self.words.next().expect("production made an extra draw")
    }

    fn next_u64(&mut self) -> u64 {
        panic!("expected u32 bounded draw, received u64 request")
    }

    fn fill_bytes(&mut self, _: &mut [u8]) {
        panic!("expected u32 bounded draw, received byte request")
    }
}
