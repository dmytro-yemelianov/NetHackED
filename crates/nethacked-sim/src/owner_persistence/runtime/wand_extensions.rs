//! Host-approved executable guest artifacts; no payload aliases or World.
use super::*;

pub(super) const CALLER_ID: &[u8] = b"NetHackED extension-capable wand caller i4 v1";
pub(super) const POLICY_PROVIDER: &str = "native-wand-action-policy-i4";
pub(super) const POLICY_ENDPOINT: &str = "select_action_i4";
pub(super) const POLICY_ID: &[u8] = b"NetHackED pure wand action policy i4 v1";
const MAX_GUESTS: usize = 8;
const MAX_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct WandGuest {
    manifest: Manifest,
    bytes: Vec<u8>,
}
impl WandGuest {
    /// This is host approval, never permission derived from saved bytes.
    pub fn approve(manifest: Manifest, bytes: Vec<u8>) -> Result<Self, OwnerBridgeError> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(kernel::Error::Limit.into());
        }
        let extension = manifest
            .extension
            .as_ref()
            .ok_or(kernel::Error::InvalidManifest)?;
        if extension.target
            != (Capability {
                provider: POLICY_PROVIDER.into(),
                endpoint: POLICY_ENDPOINT.into(),
                major: 1,
            })
            || !manifest.dependencies.is_empty()
            || !manifest.extension_points.is_empty()
            || manifest.exports.len() != 1
            || manifest.exports.get(&extension.export) != Some(&policy_endpoint())
            || manifest.id == POLICY_PROVIDER
        {
            return Err(kernel::Error::InvalidManifest.into());
        }
        Ok(Self { manifest, bytes })
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub(super) fn validate_catalog(guests: &[WandGuest]) -> Result<(), OwnerBridgeError> {
    if guests.len() > MAX_GUESTS {
        return Err(kernel::Error::Limit.into());
    }
    let mut names = BTreeSet::new();
    for guest in guests {
        if !names.insert(&guest.manifest.id) {
            return Err(kernel::Error::Duplicate.into());
        }
    }
    Ok(())
}
impl OwnerBridge {
    /// Atomically attach an already loaded, host-approved executable artifact.
    pub fn attach_wand_extension(&self, id: &str) -> Result<(), OwnerBridgeError> {
        if !self.config.wand_extensions {
            return Err(owners::Error::Unsupported.into());
        }
        let guest = self
            .config
            .extension_catalog
            .iter()
            .find(|g| g.manifest.id == id)
            .ok_or(kernel::Error::Authority)?;
        self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            if b.kernel()?.manifest(caller)? != &profile_caller(&self.config) {
                return Err(owners::Error::Unsupported.into());
            }
            let handle = b.kernel()?.registered_module(&guest.manifest)?;
            b.kernel()?.preflight_attach(handle)?;
            b.update_kernel(|k| {
                k.registered_module(&guest.manifest)
                    .and_then(|handle| k.managed_attach(handle))
            })??;
            Ok::<_, OwnerBridgeError>(())
        })??;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(())
    }
    pub fn detach_wand_extension(&self, id: &str) -> Result<(), OwnerBridgeError> {
        if !self.config.wand_extensions {
            return Err(owners::Error::Unsupported.into());
        }
        let guest = self
            .config
            .extension_catalog
            .iter()
            .find(|g| g.manifest.id == id)
            .ok_or(kernel::Error::Authority)?;
        self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            if b.kernel()?.manifest(caller)? != &profile_caller(&self.config) {
                return Err(owners::Error::Unsupported.into());
            }
            let handle = b.kernel()?.registered_module(&guest.manifest)?;
            b.kernel()?.preflight_detach(handle)?;
            b.update_kernel(|k| {
                k.registered_module(&guest.manifest)
                    .and_then(|handle| k.managed_detach(handle))
            })??;
            Ok::<_, OwnerBridgeError>(())
        })??;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(())
    }
}

// Approved guest modules are loaded dormant after owners are populated.
// Attachment/detachment is a single kernel publication on the running host.
pub(super) fn install_guests(
    kernel: &mut Kernel,
    config: &OwnerBridgeConfig,
) -> Result<(), OwnerBridgeError> {
    if config.wand_extensions {
        for guest in &config.extension_catalog {
            let handle = kernel.register_guest(guest.manifest.clone(), &guest.bytes)?;
            kernel.managed_detach(handle)?;
        }
    }
    Ok(())
}
pub(super) fn policy_endpoint() -> Endpoint {
    Endpoint {
        major: 1,
        access: Access::Read,
        max_request: wand::MAX_REQUEST_BYTES as u32,
        max_reply: wand::MAX_REQUEST_BYTES as u32,
    }
}
pub(super) fn policy_manifest() -> Manifest {
    Manifest {
        id: POLICY_PROVIDER.into(),
        exports: [(POLICY_ENDPOINT.into(), policy_endpoint())].into(),
        dependencies: vec![],
        extension_points: [POLICY_ENDPOINT.into()].into(),
        extension: None,
    }
}
pub(super) fn policy_ports() -> BTreeMap<String, NativePort> {
    let identity: NativePort = Rc::new(|bytes, _| {
        let action: wand_gameplay::WandGameplayAction =
            decode(bytes, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?;
        encode(&action, wand::MAX_REQUEST_BYTES).map_err(native_refusal)
    });
    [(POLICY_ENDPOINT.into(), identity)].into()
}
