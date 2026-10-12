use super::*;
use rand::RngCore;

/// Explicit refusal from a bounded raw-word client. It cannot be interpreted as
/// a successful sample. After a refusal this adapter makes no further requests.
#[derive(Debug, PartialEq, Eq)]
pub enum RawDrawError<E> {
    Refused(E),
    Poisoned,
    WrongReply,
}

/// Context-independent generic rand adapter. The trusted native caller supplies
/// its authenticated one-word dependency invocation, never copied RNG state.
/// Infallible RngCore methods panic on refusal; fallible raw methods expose it.
pub struct AuthenticatedWandRng<F> {
    draw: F,
    refused: bool,
}
impl<F, E> AuthenticatedWandRng<F>
where
    F: FnMut(RawWord) -> Result<OperationResult, E>,
{
    pub fn new(draw: F) -> Self {
        Self {
            draw,
            refused: false,
        }
    }
    fn word(&mut self, word: RawWord) -> Result<OperationResult, RawDrawError<E>> {
        if self.refused {
            return Err(RawDrawError::Poisoned);
        }
        let result = (self.draw)(word)
            .map_err(RawDrawError::Refused)
            .and_then(|result| {
                if matches!(
                    (&word, &result),
                    (RawWord::U32, OperationResult::Raw32(_))
                        | (RawWord::U64, OperationResult::Raw64(_))
                ) {
                    Ok(result)
                } else {
                    Err(RawDrawError::WrongReply)
                }
            });
        if result.is_err() {
            self.refused = true;
        }
        result
    }
    pub fn try_next_u32(&mut self) -> Result<u32, RawDrawError<E>> {
        match self.word(RawWord::U32)? {
            OperationResult::Raw32(n) => Ok(n),
            _ => unreachable!("validated raw reply"),
        }
    }
    pub fn try_next_u64(&mut self) -> Result<u64, RawDrawError<E>> {
        match self.word(RawWord::U64)? {
            OperationResult::Raw64(n) => Ok(n),
            _ => unreachable!("validated raw reply"),
        }
    }
}
impl<F, E> RngCore for AuthenticatedWandRng<F>
where
    F: FnMut(RawWord) -> Result<OperationResult, E>,
{
    fn next_u32(&mut self) -> u32 {
        self.try_next_u32()
            .unwrap_or_else(|_| panic!("authenticated RNG request refused"))
    }
    fn next_u64(&mut self) -> u64 {
        self.try_next_u64()
            .unwrap_or_else(|_| panic!("authenticated RNG request refused"))
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let bytes = if chunk.len() > 4 {
                self.next_u64().to_le_bytes()
            } else {
                u64::from(self.next_u32()).to_le_bytes()
            };
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}
impl OwnerBridge {
    /// Convenience facade; the subsequent native caller can use the same generic
    /// adapter with its own authenticated dependency slots and live CallerView.
    pub fn wand_rng(&self) -> impl RngCore + '_ {
        AuthenticatedWandRng::new(move |word| {
            let draws = match word {
                RawWord::U32 => 1,
                RawWord::U64 => 2,
            };
            let o = self.observe_wand(WandRead {
                query: WandQuery::Random { draws },
                expected_owner: None,
            })?;
            let witness = o.witness();
            let WandFacts::Random(reservation) = o.facts else {
                return Err(OwnerBridgeError::Owner(owners::Error::Schema));
            };
            self.operate_wand(WandOperation {
                expected_owner: witness,
                dependencies: vec![],
                command: WandOwnerCommand::Random { reservation, word },
            })
        })
    }
}
