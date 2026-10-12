//! Transport for detached approval obtained through an independently trusted host channel.
//! Canonical encoding is format validation; it does not authenticate the channel.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextRecord {
    namespace: u64,
    operation: kp::SavedOperation,
    pipeline: kp::SavedWitness,
    origin: Option<kp::SavedOrigin>,
    retained: Option<kp::SavedWitness>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostRecord {
    version: u32,
    contexts: Vec<ContextRecord>,
    checkpoint: wp::Checkpoint,
}
impl WandWorkflowApproval {
    /// Export separately from game saves to an application-controlled channel.
    /// Uses the existing owner payload ceiling and the caller's smaller limit.
    pub fn export_trusted_record(&self, limit: usize) -> Result<Vec<u8>, OwnerBridgeError> {
        let record = HostRecord {
            version: 1,
            contexts: self
                .contexts
                .iter()
                .map(|c| ContextRecord {
                    namespace: c.namespace,
                    operation: c.operation.clone(),
                    pipeline: c.pipeline.clone(),
                    origin: c.origin.clone(),
                    retained: c.retained.clone(),
                })
                .collect(),
            checkpoint: self.checkpoint.clone(),
        };
        Ok(encode(&record, limit)?)
    }
    /// Explicitly approve detached records received through a trusted host channel.
    /// The application must establish that trust independently of incoming saves.
    /// This method validates a bounded canonical format, not its authenticity.
    pub fn approve_trusted_record(bytes: &[u8], limit: usize) -> Result<Self, OwnerBridgeError> {
        let record: HostRecord = decode(bytes, limit)?;
        if record.version != 1 {
            return Err(owners::Error::Schema.into());
        }
        Ok(Self {
            contexts: record
                .contexts
                .into_iter()
                .map(|c| kp::SavedOperationContext {
                    namespace: c.namespace,
                    operation: c.operation,
                    pipeline: c.pipeline,
                    origin: c.origin,
                    retained: c.retained,
                })
                .collect(),
            checkpoint: record.checkpoint,
        })
    }
}
