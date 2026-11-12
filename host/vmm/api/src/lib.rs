//! host/vmm/api: the `MachineFactory` that runtime uses (`contracts/machine`).
//!
//! For now a stub: every call answers `UNIMPLEMENTED` with the reason `host/vmm/api#<operation>`,
//! inside a `tracing` span, so a call path through this layer can already be traced end to end.
//! The REST client of the jailed VMM process replaces it (`docs/05-vmm-and-devices.md`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use droiduse_contracts_common::{Error, ErrorCode, Result};
use droiduse_contracts_machine::{Machine, MachineFactory, MemorySource, SnapshotMeta, VmSpec};
use tracing::Instrument;

/// The component name in `Error::reason`.
const COMPONENT: &str = "host/vmm/api";

/// A `MachineFactory` that creates no VM yet: `create` and `restore` answer `UNIMPLEMENTED`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubMachineFactory;

impl StubMachineFactory {
    /// The stub factory.
    pub fn new() -> Self {
        Self
    }
}

fn unimplemented(operation: &str) -> Error {
    Error::new(ErrorCode::Unimplemented, "the VMM client is not built yet")
        .with_reason(COMPONENT, operation)
}

#[async_trait]
impl MachineFactory for StubMachineFactory {
    async fn create(&self, _spec: VmSpec) -> Result<Box<dyn Machine>> {
        async { Err(unimplemented("create")) }
            .instrument(tracing::info_span!("host/vmm/api#create"))
            .await
    }

    async fn restore(
        &self,
        _spec: VmSpec,
        _meta: SnapshotMeta,
        _mem: Box<dyn MemorySource>,
    ) -> Result<Box<dyn Machine>> {
        async { Err(unimplemented("restore")) }
            .instrument(tracing::info_span!("host/vmm/api#restore"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use droiduse_contracts_common::Labels;
    use droiduse_contracts_machine::{BootSpec, CpuTemplateRef, VsockSpec};
    use std::path::PathBuf;

    fn spec() -> VmSpec {
        VmSpec {
            vcpus: 1,
            memory_mib: 1,
            cpu_template: CpuTemplateRef(String::new()),
            boot: BootSpec::Linux {
                kernel: PathBuf::new(),
                initrd: PathBuf::new(),
                cmdline: String::new(),
            },
            disks: Vec::new(),
            vsock: VsockSpec {},
            net: Vec::new(),
            vhost_user: Vec::new(),
            vmgenid: false,
            labels: Labels::default(),
        }
    }

    struct EmptyMemory;
    #[async_trait]
    impl MemorySource for EmptyMemory {
        fn len(&self) -> u64 {
            0
        }
        async fn read_at(&self, _offset: u64, _len: usize) -> Result<Bytes> {
            Ok(Bytes::new())
        }
        async fn prefetch(&self, _ranges: &[std::ops::Range<u64>]) {}
    }

    fn expect_err(r: Result<Box<dyn Machine>>) -> Error {
        match r {
            Ok(_) => panic!("the stub created a VM"),
            Err(e) => e,
        }
    }

    #[tokio::test]
    async fn create_answers_unimplemented_with_its_reason() {
        let factory: &dyn MachineFactory = &StubMachineFactory::new();
        let e = expect_err(factory.create(spec()).await);
        assert_eq!(e.code, Some(ErrorCode::Unimplemented));
        assert_eq!(e.reason.as_deref(), Some("host/vmm/api#create"));
    }

    #[tokio::test]
    async fn restore_answers_unimplemented_with_its_reason() {
        let factory: &dyn MachineFactory = &StubMachineFactory::new();
        let e = expect_err(
            factory
                .restore(spec(), SnapshotMeta {}, Box::new(EmptyMemory))
                .await,
        );
        assert_eq!(e.code, Some(ErrorCode::Unimplemented));
        assert_eq!(e.reason.as_deref(), Some("host/vmm/api#restore"));
    }
}
