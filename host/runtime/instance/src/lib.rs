//! host/runtime/instance: the `InstanceManager` (`contracts/instance`) over a `MachineFactory`
//! (`contracts/machine`) that the entry point injects.
//!
//! For now it builds the VM definition and asks the factory for a VM, but it cannot assemble an
//! instance yet (the guest_link, frame and input channels are not built). A factory error is
//! answered unchanged, code and reason included; a VM the factory does create is destroyed again
//! and `create` answers `UNIMPLEMENTED` with the reason `host/runtime/instance#create`. No
//! instance is ever added, so `list` is empty and `close` answers `TARGET_GONE`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use droiduse_contracts_common::{Error, ErrorCode, Labels, Result};
use droiduse_contracts_instance::{CreateInstance, Instance, InstanceId, InstanceManager};
use droiduse_contracts_machine::{BootSpec, CpuTemplateRef, MachineFactory, VmSpec, VsockSpec};
use tracing::Instrument;

/// The component name in `Error::reason`.
const COMPONENT: &str = "host/runtime/instance";

/// What the VM of every new instance is built from. Runtime settings: the entry point loads them
/// and passes them in.
#[derive(Debug, Clone)]
pub struct InstanceSettings {
    /// Number of vCPUs.
    pub vcpus: u8,
    /// Guest memory (MiB).
    pub memory_mib: u32,
    /// CPU template.
    pub cpu_template: CpuTemplateRef,
    /// The guest boot image (boot image v4).
    pub boot_image: PathBuf,
    /// The guest vendor_boot image.
    pub vendor_boot_image: PathBuf,
}

/// The `InstanceManager` of this host.
pub struct LocalInstanceManager {
    factory: Arc<dyn MachineFactory>,
    settings: InstanceSettings,
    instances: Mutex<HashMap<InstanceId, Arc<dyn Instance>>>,
}

impl LocalInstanceManager {
    /// A manager that builds VMs with `factory` from `settings`.
    pub fn new(factory: Arc<dyn MachineFactory>, settings: InstanceSettings) -> Self {
        Self {
            factory,
            settings,
            instances: Mutex::new(HashMap::new()),
        }
    }

    /// The VM definition of a new instance.
    fn vm_spec(&self, labels: Labels) -> VmSpec {
        let s = &self.settings;
        VmSpec {
            vcpus: s.vcpus,
            memory_mib: s.memory_mib,
            cpu_template: s.cpu_template.clone(),
            boot: BootSpec::BootImageV4 {
                boot: s.boot_image.clone(),
                vendor_boot: s.vendor_boot_image.clone(),
                // The guest bootconfig is not built yet.
                bootconfig: Vec::new(),
            },
            disks: Vec::new(),
            vsock: VsockSpec {},
            net: Vec::new(),
            vhost_user: Vec::new(),
            // Clones must stay distinguishable (docs/07-runtime.md §4).
            vmgenid: true,
            labels,
        }
    }

    fn instances(&self) -> MutexGuard<'_, HashMap<InstanceId, Arc<dyn Instance>>> {
        self.instances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl InstanceManager for LocalInstanceManager {
    async fn create(&self, req: CreateInstance) -> Result<Arc<dyn Instance>> {
        async move {
            let machine = self.factory.create(self.vm_spec(req.labels)).await?;
            if let Err(e) = machine.destroy().await {
                tracing::warn!(error = %e, "destroying the unused VM failed");
            }
            Err(Error::new(
                ErrorCode::Unimplemented,
                "assembling an instance is not built yet",
            )
            .with_reason(COMPONENT, "create"))
        }
        .instrument(tracing::info_span!("host/runtime/instance#create"))
        .await
    }

    fn list(&self) -> Vec<Arc<dyn Instance>> {
        self.instances().values().cloned().collect()
    }

    fn get(&self, id: &InstanceId) -> Option<Arc<dyn Instance>> {
        self.instances().get(id).cloned()
    }

    async fn close(&self, id: &InstanceId) -> Result<()> {
        let known = self.instances().contains_key(id);
        let e = if known {
            Error::new(
                ErrorCode::Unimplemented,
                "stopping an instance is not built yet",
            )
        } else {
            Error::new(ErrorCode::TargetGone, format!("no instance {}", id.0))
        };
        Err(e.with_reason(COMPONENT, "close"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::EventStream;
    use droiduse_contracts_machine::{Machine, MemorySource, SnapshotMeta, SnapshotSink, VmEvent};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn settings() -> InstanceSettings {
        InstanceSettings {
            vcpus: 2,
            memory_mib: 2048,
            cpu_template: CpuTemplateRef("t".to_owned()),
            boot_image: PathBuf::from("boot.img"),
            vendor_boot_image: PathBuf::from("vendor_boot.img"),
        }
    }

    fn expect_err(r: Result<Arc<dyn Instance>>) -> Error {
        match r {
            Ok(_) => panic!("an instance was created"),
            Err(e) => e,
        }
    }

    /// A factory whose `create` answers a fixed error.
    struct FailingFactory(Error);
    #[async_trait]
    impl MachineFactory for FailingFactory {
        async fn create(&self, _spec: VmSpec) -> Result<Box<dyn Machine>> {
            Err(self.0.clone())
        }
        async fn restore(
            &self,
            _spec: VmSpec,
            _meta: SnapshotMeta,
            _mem: Box<dyn MemorySource>,
        ) -> Result<Box<dyn Machine>> {
            Err(self.0.clone())
        }
    }

    /// A factory that creates a VM, records the spec, and records that the VM was destroyed.
    #[derive(Default)]
    struct RecordingFactory {
        spec: Mutex<Option<VmSpec>>,
        destroyed: Arc<AtomicBool>,
    }
    struct RecordingMachine(Arc<AtomicBool>);
    #[async_trait]
    impl Machine for RecordingMachine {
        async fn start(&self) -> Result<()> {
            Ok(())
        }
        async fn pause(&self) -> Result<()> {
            Ok(())
        }
        async fn resume(&self) -> Result<()> {
            Ok(())
        }
        async fn snapshot(&self, _out: SnapshotSink) -> Result<SnapshotMeta> {
            Ok(SnapshotMeta {})
        }
        async fn bump_generation(&self) -> Result<()> {
            Ok(())
        }
        async fn destroy(self: Box<Self>) -> Result<()> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn events(&self) -> EventStream<VmEvent> {
            unimplemented!("not used by these tests")
        }
    }
    #[async_trait]
    impl MachineFactory for RecordingFactory {
        async fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>> {
            *self.spec.lock().unwrap() = Some(spec);
            Ok(Box::new(RecordingMachine(self.destroyed.clone())))
        }
        async fn restore(
            &self,
            _spec: VmSpec,
            _meta: SnapshotMeta,
            _mem: Box<dyn MemorySource>,
        ) -> Result<Box<dyn Machine>> {
            unimplemented!("not used by these tests")
        }
    }

    #[tokio::test]
    async fn a_factory_error_is_answered_unchanged_and_leaves_no_instance() {
        let from_vmm = Error::new(ErrorCode::Unimplemented, "the VMM client is not built yet")
            .with_reason("host/vmm/api", "create");
        let m = LocalInstanceManager::new(Arc::new(FailingFactory(from_vmm.clone())), settings());
        let e = expect_err(m.create(CreateInstance::default()).await);
        assert_eq!(e, from_vmm);
        assert!(m.list().is_empty());
    }

    #[tokio::test]
    async fn a_created_vm_is_destroyed_again_and_leaves_no_instance() {
        let factory = Arc::new(RecordingFactory::default());
        let m = LocalInstanceManager::new(factory.clone(), settings());
        let mut labels = Labels::default();
        labels.0.insert("k".to_owned(), "v".to_owned());
        let e = expect_err(m.create(CreateInstance::new(labels.clone())).await);
        assert_eq!(e.code, Some(ErrorCode::Unimplemented));
        assert_eq!(e.reason.as_deref(), Some("host/runtime/instance#create"));
        assert!(factory.destroyed.load(Ordering::SeqCst));
        assert!(m.list().is_empty());

        let spec = factory
            .spec
            .lock()
            .unwrap()
            .take()
            .expect("create was called");
        assert_eq!((spec.vcpus, spec.memory_mib), (2, 2048));
        assert_eq!(spec.cpu_template, CpuTemplateRef("t".to_owned()));
        assert!(spec.vmgenid);
        assert_eq!(spec.labels, labels);
        match spec.boot {
            BootSpec::BootImageV4 {
                boot,
                vendor_boot,
                bootconfig,
            } => {
                assert_eq!(boot, PathBuf::from("boot.img"));
                assert_eq!(vendor_boot, PathBuf::from("vendor_boot.img"));
                assert!(bootconfig.is_empty());
            }
            other => panic!("unexpected boot {other:?}"),
        }
    }

    #[tokio::test]
    async fn closing_an_unknown_instance_is_target_gone() {
        let m = LocalInstanceManager::new(Arc::new(RecordingFactory::default()), settings());
        let id = InstanceId("i1".to_owned());
        assert!(m.get(&id).is_none());
        let e = m.close(&id).await.unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::TargetGone));
        assert_eq!(e.reason.as_deref(), Some("host/runtime/instance#close"));
    }
}
