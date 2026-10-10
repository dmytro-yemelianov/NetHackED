//! Opt-in live workflows and independently retained host approval records.
use super::*;
use nethacked_persistence::workflows::{self as wf, persistence as wp};

const PROVIDER: &str = "native-wand-workflow-i4";
const STEP: &str = "step_i4";
pub(super) const EXECUTE: &str = "execute_i4";
const START: &str = "start_i4";
const CODE: &[u8] = b"NetHackED retained wand workflow i4 v1";
pub(super) fn code(c: &OwnerBridgeConfig) -> &'static [u8] {
    if c.wand_workflow_replies {
        b"NetHackED retained wand replies i5 v1"
    } else {
        CODE
    }
}
fn program_identity(c: &OwnerBridgeConfig) -> wp::Identity {
    identity(if c.wand_workflow_replies {
        "wand-workflow-program-i5"
    } else {
        "wand-workflow-program-i4"
    })
}

fn identity(name: &str) -> wp::Identity {
    wp::Identity {
        name: name.into(),
        version: 1,
        artifact: [243; 32],
    }
}
fn authority() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::All,
    }
}
pub(super) fn manifest(c: &OwnerBridgeConfig) -> Manifest {
    Manifest {
        id: if c.wand_workflow_replies {
            "native-wand-workflow-replies-i5".into()
        } else {
            PROVIDER.into()
        },
        exports: [(
            STEP.into(),
            Endpoint {
                major: 1,
                access: Access::Write,
                max_request: wf::Limits::default().wire_bytes as u32,
                max_reply: wf::Limits::default().wire_bytes as u32,
            },
        )]
        .into(),
        dependencies: vec![kernel::Dependency {
            capability: Capability {
                provider: profile_caller(c).id,
                endpoint: wand_gameplay::ENDPOINT.into(),
                major: 1,
            },
            access: Access::Write,
        }],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
pub(super) fn artifact(c: &OwnerBridgeConfig) -> kernel::Result<[u8; 32]> {
    artifact_for(manifest(c), code(c), c)
}
fn artifact_for(
    manifest: Manifest,
    code: &[u8],
    c: &OwnerBridgeConfig,
) -> kernel::Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(
        &serde_json::to_value((manifest, profile_limits(c), Option::<[u8; 32]>::None))
            .map_err(|_| kernel::Error::NativeRefusal)?,
    )
    .map_err(|_| kernel::Error::NativeRefusal)?;
    let mut hash = Sha256::new();
    for part in [b"native-v1".as_slice(), bytes.as_slice(), code] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    Ok(hash.finalize().into())
}
pub(super) fn allows_gameplay_owner(c: &OwnerBridgeConfig, view: &CallerView, phase: &str) -> bool {
    c.wand_workflows
        && phase == EXECUTE
        && artifact_for(profile_caller(c), wand_commands::caller_id(c), c)
            .ok()
            .is_some_and(|a| view.caller_artifact() == Some(a))
}
pub(super) fn ports(c: &OwnerBridgeConfig) -> BTreeMap<String, NativePort> {
    let detailed = c.wand_workflow_replies;
    let step: NativePort = Rc::new(move |bytes, view| {
        // Workflows' request wire uses StepRequest field order, independently
        // of the sorted owner-payload encoding used by this application.
        if bytes.len() > wf::Limits::default().wire_bytes {
            return Err(kernel::Error::Buffer);
        }
        let request: wf::StepRequest =
            serde_json::from_slice(bytes).map_err(|_| kernel::Error::NativeRefusal)?;
        if serde_json::to_vec(&request).map_err(|_| kernel::Error::NativeRefusal)? != bytes {
            return Err(kernel::Error::NativeRefusal);
        }
        if !request.input.is_empty() || view.operation().is_none_or(|o| o.phase() != request.phase)
        {
            return Err(kernel::Error::Authority);
        }
        let action: wand_gameplay::WandGameplayAction =
            decode(&request.state, wf::Limits::default().state_bytes).map_err(native_refusal)?;
        let outcome = match request.phase.as_str() {
            START if request.sequence == 0 => wf::Outcome::Suspend {
                next: EXECUTE.into(),
                prompt: b"wand-ready".to_vec(),
                state: request.state,
                events: vec![],
            },
            EXECUTE if request.sequence == 2 => {
                let reply = view.call_dependency(
                    0,
                    &encode(&action, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?,
                )?;
                let reply: wand_gameplay::WandGameplayReply =
                    decode(&reply, 1024 * 1024).map_err(native_refusal)?;
                if detailed {
                    let (result, events) =
                        completion(&reply, &request.state).map_err(native_refusal)?;
                    wf::Outcome::Finish {
                        result,
                        state: request.state,
                        events,
                    }
                } else {
                    // Preserve the exact I4 boolean-only executable protocol.
                    wf::Outcome::Finish {
                        result: vec![u8::from(reply.error.is_none())],
                        state: request.state,
                        events: vec![],
                    }
                }
            }
            _ => return Err(kernel::Error::Authority),
        };
        encode(&outcome, wf::Limits::default().wire_bytes).map_err(native_refusal)
    });
    [(STEP.into(), step)].into()
}

// Full replies use the existing finite workflow trace, not an unbounded cache.
// The descriptor always fits value_bytes; oversize outcomes explicitly retain
// only their execution status/size, so bool callers retain summary semantics.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    success: bool,
    reply_bytes: usize,
    digest: [u8; 32],
    overflow: bool,
}
fn completion(
    reply: &wand_gameplay::WandGameplayReply,
    state: &[u8],
) -> owners::Result<(Vec<u8>, Vec<Vec<u8>>)> {
    use sha2::{Digest, Sha256};
    let bytes = encode(reply, 1024 * 1024)?;
    let limits = wf::Limits::default();
    let overflow = bytes.len() > limits.events * limits.event_bytes;
    let mut descriptor = Completion {
        success: reply.error.is_none(),
        reply_bytes: bytes.len(),
        digest: Sha256::digest(&bytes).into(),
        overflow,
    };
    let mut events = if overflow {
        vec![]
    } else {
        bytes
            .chunks(limits.event_bytes)
            .map(<[u8]>::to_vec)
            .collect()
    };
    let result = encode(&descriptor, limits.value_bytes)?;
    // Vec<u8> uses JSON integer arrays: a trace fitting its byte budget can
    // still exceed the complete outcome wire budget, including state/framing.
    if let Err(error) = encode(
        &wf::Outcome::Finish {
            result: result.clone(),
            state: state.to_vec(),
            events: events.clone(),
        },
        limits.wire_bytes,
    ) {
        if error != owners::Error::Limit {
            return Err(error);
        }
        descriptor.overflow = true;
        events.clear();
    }
    Ok((encode(&descriptor, limits.value_bytes)?, events))
}
fn descriptor(frame: &wf::Frame) -> Result<Completion, OwnerBridgeError> {
    let wf::Status::Finished(result) = &frame.status else {
        return Err(OwnerBridgeError::WorkflowStatus(frame.status.clone()));
    };
    let value: Completion = decode(result, wf::Limits::default().value_bytes)?;
    if encode(&value, wf::Limits::default().value_bytes)? != *result {
        return Err(owners::Error::Schema.into());
    }
    Ok(value)
}
fn retained_reply(frame: &wf::Frame) -> Result<wand_gameplay::WandGameplayReply, OwnerBridgeError> {
    use sha2::{Digest, Sha256};
    let value = descriptor(frame)?;
    let limits = wf::Limits::default();
    if value.overflow
        || value.reply_bytes == 0
        || value.reply_bytes > limits.events * limits.event_bytes
    {
        return Err(wf::Error::Bounds.into());
    }
    if frame.trace.len() != value.reply_bytes.div_ceil(limits.event_bytes)
        || frame.trace.iter().enumerate().any(|(i, bytes)| {
            bytes.len() != (value.reply_bytes - i * limits.event_bytes).min(limits.event_bytes)
        })
    {
        return Err(owners::Error::Schema.into());
    }
    let bytes: Vec<u8> = frame.trace.iter().flatten().copied().collect();
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != value.digest {
        return Err(owners::Error::Schema.into());
    }
    let reply: wand_gameplay::WandGameplayReply =
        decode(&bytes, limits.events * limits.event_bytes)?;
    if encode(&reply, limits.events * limits.event_bytes)? != bytes
        || reply.error.is_none() != value.success
    {
        return Err(owners::Error::Schema.into());
    }
    Ok(reply)
}
fn effective_config(
    b: &session::HostBundle,
    c: &OwnerBridgeConfig,
    provider: Option<kernel::ModuleHandle>,
) -> Result<OwnerBridgeConfig, OwnerBridgeError> {
    for profile in approved_profiles(c)
        .into_iter()
        .filter(|p| p.wand_workflows)
    {
        if let Ok(actual) = b.kernel()?.registered_module(&manifest(&profile)) {
            if provider.is_none_or(|wanted| wanted == actual) {
                return Ok(profile);
            }
        }
    }
    Err(owners::Error::Unsupported.into())
}

struct ActionCodec;
impl wp::StateCodec for ActionCodec {
    fn identity(&self) -> wp::Identity {
        identity("wand-action-state-i4")
    }
    fn validate(&self, bytes: &[u8], limit: usize) -> wf::Result<Vec<wp::Reference>> {
        let action: wand_gameplay::WandGameplayAction =
            decode(bytes, limit.min(wf::Limits::default().state_bytes))
                .map_err(|_| wf::Error::Bounds)?;
        if encode(&action, limit).map_err(|_| wf::Error::Bounds)? != bytes {
            return Err(wf::Error::Bounds);
        }
        // ItemId refers to the preserved Objects logical arena, not kernel handles.
        Ok(vec![])
    }
    fn remap(&self, bytes: &[u8], _: &wp::LocalRemap<'_>, limit: usize) -> wf::Result<Vec<u8>> {
        self.validate(bytes, limit)?;
        Ok(bytes.to_vec())
    }
}

/// Created only from this trusted host's live execution observations, separately
/// from save bytes. Contains detached records, never runtime/owner aliases.
#[derive(Clone)]
pub struct WandWorkflowApproval {
    contexts: Vec<kp::SavedOperationContext>,
    checkpoint: wp::Checkpoint,
}
impl wp::FrameAuthority for WandWorkflowApproval {
    fn approve(
        &self,
        namespace: u64,
        frame: &wp::FrameRow,
        participants: &[wp::ParticipantRow],
    ) -> wf::Result<()> {
        if namespace == self.checkpoint.namespace
            && self.checkpoint.frames.contains(frame)
            && participants == self.checkpoint.participants
        {
            Ok(())
        } else {
            Err(wf::Error::Status)
        }
    }
}
impl session::WorkflowFactory for WandWorkflowApproval {
    fn catalog(&self, handles: Rc<session::Handles>) -> Result<wp::RestoreCatalog, pr::Error> {
        let mut catalog = wp::RestoreCatalog::default();
        let mut programs = BTreeSet::new();
        for frame in &self.checkpoint.frames {
            if programs.insert(frame.program.identity.clone()) {
                catalog
                    .program(frame.program.clone(), Rc::new(ActionCodec))
                    .map_err(|_| pr::Error::FactoryRefused)?;
            }
        }
        if !self.checkpoint.participants.is_empty() {
            catalog
                .participant(identity("wand-owner-cleanup-i4"), move |id| {
                    let participant = handles.owner_cleanup(|host| Rc::new(host))?;
                    if participant.identity()? != id {
                        return Err(wf::Error::Kernel(kernel::Error::Authority));
                    }
                    Ok(participant)
                })
                .map_err(|_| pr::Error::FactoryRefused)?;
        }
        Ok(catalog)
    }
}
struct EmptyApproval;
impl session::WorkflowFactory for EmptyApproval {
    fn catalog(&self, _: Rc<session::Handles>) -> Result<wp::RestoreCatalog, pr::Error> {
        Ok(wp::RestoreCatalog::default())
    }
}
impl wp::FrameAuthority for EmptyApproval {}
pub(super) fn restore_approval(c: &OwnerBridgeConfig) -> Option<session::WorkflowApproval> {
    if !c.wand_workflows {
        return None;
    }
    Some(if let Some(a) = &c.workflow_approval {
        (
            Rc::new(a.clone()),
            Rc::new(a.clone()),
            wf::Limits::default(),
        )
    } else {
        (
            Rc::new(EmptyApproval),
            Rc::new(EmptyApproval),
            wf::Limits::default(),
        )
    })
}
pub(super) fn approve_context(
    c: &OwnerBridgeConfig,
    context: &kp::SavedOperationContext,
) -> kernel::Result<()> {
    if c.wand_workflows
        && c.workflow_approval
            .as_ref()
            .is_some_and(|a| a.contexts.contains(context))
    {
        Ok(())
    } else {
        Err(kernel::Error::Authority)
    }
}
pub(super) fn approve_grant(
    c: &OwnerBridgeConfig,
    caller: &kp::SavedModule,
    slot: usize,
    target: &kp::SavedModule,
    requested: &Authority,
) -> kernel::Result<Authority> {
    let Scope::Keys(keys) = &requested.scope else {
        return Err(kernel::Error::Authority);
    };
    let namespace = &keys.first().ok_or(kernel::Error::Authority)?.owner;
    if c.wand_workflows
        && caller.manifest == manifest(c)
        && slot == 0
        && target.manifest == profile_caller(c)
        && requested.access == Access::Write
        && namespace
            .strip_prefix("owners-")
            .is_some_and(|s| s.parse::<u64>().is_ok())
        && keys.iter().all(|r| r.owner == *namespace)
        && keys.iter().map(|r| r.key.clone()).collect::<BTreeSet<_>>()
            == wand_commands::gameplay_grant_names(c)
    {
        Ok(requested.clone())
    } else {
        Err(kernel::Error::Authority)
    }
}
pub(super) fn install(
    kernel: &mut Kernel,
    c: &OwnerBridgeConfig,
    keys: &BTreeSet<Resource>,
) -> Result<session::Services, OwnerBridgeError> {
    if !c.wand_workflows {
        return Ok(session::Services::default());
    }
    let provider = kernel.register_native(manifest(c), code(c), ports(c))?;
    kernel.issue_grant(
        provider,
        0,
        Authority {
            access: Access::Write,
            scope: Scope::Keys(keys.clone()),
        },
    )?;
    let workflows = wf::Workflows::new(wf::Limits::default())?;
    let catalog = wp::CaptureCatalog::new(&workflows);
    Ok(session::Services {
        workflows: Some((workflows, catalog)),
        ..Default::default()
    })
}
fn program(provider: kernel::ModuleHandle) -> wf::Program {
    wf::Program {
        provider,
        endpoint: STEP.into(),
        authority: authority(),
        initial: START.into(),
        phases: [START.into(), EXECUTE.into()].into(),
        allowed: [(START.into(), EXECUTE.into())].into(),
    }
}
impl OwnerBridge {
    /// Withdraw independent checkpoint approval for subsequent restore attempts.
    /// Already admitted execution remains bound to its retained code.
    pub fn revoke_wand_workflow_approval(&mut self) -> Result<(), OwnerBridgeError> {
        let mut config = self.config.clone();
        config.workflow_approval = None;
        let configuration = restore_configuration(&config)?;
        self.session
            .replace_configuration(&self.key, configuration)?;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        self.config = config;
        Ok(())
    }
    pub fn pause_wand(
        &self,
        action: wand_gameplay::WandGameplayAction,
    ) -> Result<wf::Ticket, OwnerBridgeError> {
        if !self.config.wand_workflows {
            return Err(owners::Error::Unsupported.into());
        }
        let state = encode(&action, wf::Limits::default().state_bytes)?;
        let ticket = self.session.with_runtime(&self.credential.borrow(), |b| {
            let effective = effective_config(b, &self.config, None)?;
            let provider = b.kernel()?.registered_module(&manifest(&effective))?;
            let (workflows, catalog) = b
                .services()
                .workflows
                .as_ref()
                .ok_or(owners::Error::Unsupported)?;
            let cleanup: Rc<dyn wf::CleanupParticipant> = Rc::new(b.host().clone());
            let id = b.update_kernel(|k| {
                workflows.start(k, program(provider), state, vec![cleanup.clone()])
            })??;
            catalog.enroll_program(id, program_identity(&effective), Rc::new(ActionCodec))?;
            catalog.enroll_participant(identity("wand-owner-cleanup-i4"), cleanup)?;
            let status = workflows.step(&*b.kernel()?, id, &[])?;
            if !matches!(status, wf::Status::Suspended(_)) {
                return Err(OwnerBridgeError::WorkflowStatus(status));
            }
            Ok::<_, OwnerBridgeError>(workflows.ticket(id)?)
        })??;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(ticket)
    }
    pub fn pending_wands(&self) -> Result<Vec<wf::Ticket>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            let (workflows, _) = b
                .services()
                .workflows
                .as_ref()
                .ok_or(owners::Error::Unsupported)?;
            Ok::<_, OwnerBridgeError>(
                workflows
                    .operations()?
                    .into_iter()
                    .filter_map(|id| workflows.ticket(id).ok())
                    .collect(),
            )
        })?
    }
    /// Compatibility summary for both exact historical and detailed workflows.
    pub fn resume_wand(&self, ticket: &wf::Ticket) -> Result<bool, OwnerBridgeError> {
        let (frame, detailed) = self.resume_wand_frame(ticket, false)?;
        if detailed {
            Ok(descriptor(&frame)?.success)
        } else if let wf::Status::Finished(result) = frame.status {
            Ok(result == [1])
        } else {
            Err(OwnerBridgeError::WorkflowStatus(frame.status))
        }
    }
    /// Resume and return the actual retained reply, including its ordered events.
    /// Historical boolean-only frames are refused before consuming their ticket.
    /// A completion exceeding fixed trace bounds returns Bounds after its already
    /// committed gameplay prefix; it never returns truncated or invented events.
    pub fn resume_wand_reply(
        &self,
        ticket: &wf::Ticket,
    ) -> Result<wand_gameplay::WandGameplayReply, OwnerBridgeError> {
        let (frame, _) = self.resume_wand_frame(ticket, true)?;
        retained_reply(&frame)
    }
    fn resume_wand_frame(
        &self,
        ticket: &wf::Ticket,
        require_reply: bool,
    ) -> Result<(wf::Frame, bool), OwnerBridgeError> {
        let frame = self.session.with_runtime(&self.credential.borrow(), |b| {
            let (workflows, _) = b
                .services()
                .workflows
                .as_ref()
                .ok_or(owners::Error::Unsupported)?;
            let before = workflows.inspect(ticket.operation)?;
            let effective = effective_config(b, &self.config, Some(before.program.provider))?;
            if require_reply && !effective.wand_workflow_replies {
                return Err(owners::Error::Unsupported.into());
            }
            workflows.resume(ticket)?;
            let status = workflows.step(&*b.kernel()?, ticket.operation, &[])?;
            if !matches!(status, wf::Status::Finished(_)) {
                return Err(OwnerBridgeError::WorkflowStatus(status));
            }
            Ok::<_, OwnerBridgeError>((
                workflows.inspect(ticket.operation)?,
                effective.wand_workflow_replies,
            ))
        })??;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(frame)
    }
    /// Explicit host approval of observed live paused/finished operations.
    /// The resulting record must remain independent of incoming saves.
    pub fn approve_live_wands(&self) -> Result<WandWorkflowApproval, OwnerBridgeError> {
        if !self.config.wand_workflows {
            return Err(owners::Error::Unsupported.into());
        }
        let evidence = self.session.live_workflow_evidence(&self.key)?;
        let effective = self.session.with_runtime(&self.credential.borrow(), |b| {
            effective_config(b, &self.config, None)
        })??;
        let approved_artifact = artifact(&effective)?;
        if evidence.checkpoint.frames.iter().any(|f| {
            f.program.artifact != approved_artifact
                || f.program.endpoint != STEP
                || f.program.identity != program_identity(&effective)
        }) {
            return Err(kernel::Error::Authority.into());
        }
        Ok(WandWorkflowApproval {
            contexts: evidence.contexts,
            checkpoint: evidence.checkpoint,
        })
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    fn frame() -> wf::Frame {
        let w = SimulationWorld::new_with_seed(53);
        let cfg = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_extensions(vec![])
            .unwrap()
            .with_wand_workflows()
            .unwrap()
            .with_wand_workflow_replies()
            .unwrap();
        let json = w.to_save_json().unwrap();
        let b = OwnerBridge::import_native(&json, json.len(), cfg).unwrap();
        let ticket = b
            .pause_wand(wand_gameplay::WandGameplayAction::Cancel)
            .unwrap();
        let frame = b
            .session
            .with_runtime(&b.credential.borrow(), |bundle| {
                bundle
                    .services()
                    .workflows
                    .as_ref()
                    .unwrap()
                    .0
                    .inspect(ticket.operation)
            })
            .unwrap()
            .unwrap();
        frame
    }
    #[test]
    fn completion_chunks_refuse_truncation_reordering_and_oversize_without_fabricating_reply() {
        let reply = wand_gameplay::WandGameplayReply {
            events: vec![crate::GameEvent::LogMessage {
                text: "x".repeat(700),
            }],
            error: None,
            dependency_calls: 11,
        };
        let (result, trace) = completion(&reply, &[]).unwrap();
        assert!(trace.len() > 1);
        let mut f = frame();
        f.status = wf::Status::Finished(result);
        f.trace = trace;
        assert_eq!(
            encode(&retained_reply(&f).unwrap(), 4096).unwrap(),
            encode(&reply, 4096).unwrap()
        );
        let mut bad = f.clone();
        bad.trace.pop();
        assert!(retained_reply(&bad).is_err());
        let mut bad = f.clone();
        bad.trace.swap(0, 1);
        assert!(retained_reply(&bad).is_err());
        let mut bad = f.clone();
        bad.trace[0][0] ^= 1;
        assert!(retained_reply(&bad).is_err());
        // A reply fitting trace bytes may overflow the complete JSON outcome.
        let wire_reply = wand_gameplay::WandGameplayReply {
            events: vec![crate::GameEvent::LogMessage {
                text: "x".repeat(3800),
            }],
            error: None,
            dependency_calls: 11,
        };
        let (result, trace) = completion(&wire_reply, &vec![120; 1024]).unwrap();
        f.status = wf::Status::Finished(result.clone());
        f.trace = trace.clone();
        assert!(descriptor(&f).unwrap().overflow);
        assert!(trace.is_empty());
        assert!(encode(
            &wf::Outcome::Finish {
                result,
                state: vec![120; 1024],
                events: trace
            },
            wf::Limits::default().wire_bytes
        )
        .is_ok());
        let reply = wand_gameplay::WandGameplayReply {
            events: vec![crate::GameEvent::LogMessage {
                text: "x".repeat(5000),
            }],
            error: None,
            dependency_calls: 11,
        };
        let (result, trace) = completion(&reply, &[]).unwrap();
        f.status = wf::Status::Finished(result);
        f.trace = trace;
        assert!(descriptor(&f).unwrap().overflow);
        assert!(descriptor(&f).unwrap().success);
        assert!(f.trace.is_empty());
        assert!(matches!(
            retained_reply(&f),
            Err(OwnerBridgeError::Workflow(wf::Error::Bounds))
        ));
    }
}

mod trusted_record;
