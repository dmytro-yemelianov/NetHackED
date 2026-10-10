//! Opt-in live workflows and independently retained host approval records.
use super::*;
use nethacked_persistence::workflows::{self as wf, persistence as wp};

const PROVIDER: &str = "native-wand-workflow-i4";
const STEP: &str = "step_i4";
pub(super) const EXECUTE: &str = "execute_i4";
const START: &str = "start_i4";
pub(super) const CODE: &[u8] = b"NetHackED retained wand workflow i4 v1";

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
        id: PROVIDER.into(),
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
    artifact_for(manifest(c), CODE, c)
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
pub(super) fn ports() -> BTreeMap<String, NativePort> {
    let step: NativePort = Rc::new(|bytes, view| {
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
                // Workflows expose a bounded execution summary; ordinary command
                // replies and their event streams retain the existing public API.
                wf::Outcome::Finish {
                    result: vec![u8::from(reply.error.is_none())],
                    state: request.state,
                    events: vec![],
                }
            }
            _ => return Err(kernel::Error::Authority),
        };
        encode(&outcome, wf::Limits::default().wire_bytes).map_err(native_refusal)
    });
    [(STEP.into(), step)].into()
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
    let provider = kernel.register_native(manifest(c), CODE, ports())?;
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
            let provider = b.kernel()?.registered_module(&manifest(&self.config))?;
            let (workflows, catalog) = b
                .services()
                .workflows
                .as_ref()
                .ok_or(owners::Error::Unsupported)?;
            let cleanup: Rc<dyn wf::CleanupParticipant> = Rc::new(b.host().clone());
            let id = b.update_kernel(|k| {
                workflows.start(k, program(provider), state, vec![cleanup.clone()])
            })??;
            catalog.enroll_program(
                id,
                identity("wand-workflow-program-i4"),
                Rc::new(ActionCodec),
            )?;
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
    pub fn resume_wand(&self, ticket: &wf::Ticket) -> Result<bool, OwnerBridgeError> {
        let success = self.session.with_runtime(&self.credential.borrow(), |b| {
            let (workflows, _) = b
                .services()
                .workflows
                .as_ref()
                .ok_or(owners::Error::Unsupported)?;
            workflows.resume(ticket)?;
            let status = workflows.step(&*b.kernel()?, ticket.operation, &[])?;
            if let wf::Status::Finished(result) = status {
                Ok::<_, OwnerBridgeError>(result == [1])
            } else {
                Err(OwnerBridgeError::WorkflowStatus(status))
            }
        })??;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(success)
    }
    /// Explicit host approval of observed live paused/finished operations.
    /// The resulting record must remain independent of incoming saves.
    pub fn approve_live_wands(&self) -> Result<WandWorkflowApproval, OwnerBridgeError> {
        if !self.config.wand_workflows {
            return Err(owners::Error::Unsupported.into());
        }
        let evidence = self.session.live_workflow_evidence(&self.key)?;
        let approved_artifact = artifact(&self.config)?;
        if evidence.checkpoint.frames.iter().any(|f| {
            f.program.artifact != approved_artifact
                || f.program.endpoint != STEP
                || f.program.identity != identity("wand-workflow-program-i4")
        }) {
            return Err(kernel::Error::Authority.into());
        }
        Ok(WandWorkflowApproval {
            contexts: evidence.contexts,
            checkpoint: evidence.checkpoint,
        })
    }
}
