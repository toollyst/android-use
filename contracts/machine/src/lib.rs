//! machine contract: runtime ↔ vmm.
//!
//! Definition and control of one VM. Details: `docs/03-contracts.md` machine.
//!
//! **No Android concepts.** `host/vmm` uses this crate, so no identifier or string that handles
//! an Android concept goes here (`LAYERS.toml` `[forbidden]`). Guest bootconfig values are built
//! by `host/runtime/instance` and passed on as opaque key=value pairs; vmm does not interpret
//! them.
//!
//! The VMM is a separate process inside a jail. On the runtime side, the REST client of
//! `host/vmm/api` implements this trait (in-process implementations are for tests). The schema
//! of that extension REST API is not in this crate (undecided).
//!
//! Async methods are declared with `async-trait`. They can be used as trait objects (dyn),
//! assume no particular async runtime, and return `Send` futures. Dropping a future cancels
//! that call.
//!
//! A type marked `PLACEHOLDER` is one only named by the documentation. Its structure and
//! meaning are undecided.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use async_trait::async_trait;
use droiduse_contracts_common::{EventStream, Labels, Result};

/// The snapshot metadata and memory source that `MachineFactory::restore` takes.
///
/// They are defined in the `spi` contract (the same types as the `SnapshotStore` and
/// `MemorySource` SPIs).
pub use droiduse_contracts_spi::{MemorySource, SnapshotMeta};

// ---------------------------------------------------------------------------
// VM definition
// ---------------------------------------------------------------------------

/// Definition of one VM.
#[derive(Debug, Clone)]
pub struct VmSpec {
    /// Number of vCPUs.
    pub vcpus: u8,
    /// Guest memory (MiB).
    pub memory_mib: u32,
    /// CPU template. Required for snapshot portability.
    pub cpu_template: CpuTemplateRef,
    /// How the VM boots.
    pub boot: BootSpec,
    /// Disks. GPT + CoW layers (base ro / snapshot / profile).
    pub disks: Vec<CompositeDiskSpec>,
    /// vsock.
    pub vsock: VsockSpec,
    /// Networks. vhost-user-net backend sockets.
    pub net: Vec<NetSpec>,
    /// vhost-user devices. `{kind, socket, shm_regions}`.
    pub vhost_user: Vec<VhostUserDeviceSpec>,
    /// Whether the VMGenID device is used.
    pub vmgenid: bool,
    /// Opaque map. The core does not interpret it.
    pub labels: Labels,
}

/// How the VM boots.
#[derive(Debug, Clone)]
pub enum BootSpec {
    /// Plain Linux boot.
    Linux {
        /// Kernel image.
        kernel: PathBuf,
        /// initrd.
        initrd: PathBuf,
        /// Kernel cmdline.
        cmdline: String,
    },
    /// Boot from the boot image v4 + vendor_boot **container format**. The name is the name of
    /// the format.
    BootImageV4 {
        /// boot image.
        boot: PathBuf,
        /// vendor_boot image.
        vendor_boot: PathBuf,
        /// bootconfig. An opaque list of key=value pairs. vmm does not interpret it.
        /// Rules for order, duplicate keys and value escaping are undecided.
        bootconfig: Vec<(String, String)>,
    },
}

/// Reference to a CPU template (pins the CPU features the guest sees).
///
/// PLACEHOLDER: representation undecided (the string only means "an opaque value").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CpuTemplateRef(pub String);

/// GPT composite disk + CoW layers (base ro / snapshot / profile).
///
/// PLACEHOLDER: structure undecided. The named partition layout is in
/// `docs/05-vmm-and-devices.md` §3 `host/vmm/disk`.
#[derive(Debug, Clone)]
pub struct CompositeDiskSpec {}

/// vsock settings. PLACEHOLDER: structure undecided.
#[derive(Debug, Clone)]
pub struct VsockSpec {}

/// One network (vhost-user-net backend socket). PLACEHOLDER: structure undecided.
#[derive(Debug, Clone)]
pub struct NetSpec {}

/// One vhost-user device. Documentation: `{kind, socket, shm_regions}`.
#[derive(Debug, Clone)]
pub struct VhostUserDeviceSpec {
    /// Device kind.
    pub kind: VhostUserDeviceKind,
    /// Unix socket path of the vhost-user backend.
    pub socket: PathBuf,
    /// virtio shared memory regions (gfxstream blob resources, etc.).
    pub shm_regions: Vec<ShmRegionSpec>,
}

/// vhost-user device kind.
///
/// PLACEHOLDER: the documentation has only the field name (`kind`). The value set and
/// representation are undecided.
#[derive(Debug, Clone)]
pub struct VhostUserDeviceKind {}

/// One virtio shared memory region.
///
/// PLACEHOLDER: the documentation has only the field name (`shm_regions`). Structure undecided.
#[derive(Debug, Clone)]
pub struct ShmRegionSpec {}

// ---------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------

/// The VM creation path. Kept apart from `Machine` so that both can be used as trait objects
/// (dyn).
///
/// **Cancellation**: if `create` or `restore` is cancelled midway (future dropped), the VM being
/// built has failed. No VM is returned to the caller, and runtime cleans up the half-built VMM
/// process and its resources. That instance ends in `Failed` (instance contract).
#[async_trait]
pub trait MachineFactory: Send + Sync {
    /// Creates a new VM.
    async fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>>;
    /// Restores a VM from a snapshot.
    async fn restore(
        &self,
        spec: VmSpec,
        meta: SnapshotMeta,
        mem: Box<dyn MemorySource>,
    ) -> Result<Box<dyn Machine>>;
}

/// Control of one VM.
///
/// **Cancellation**: if an async method (a lifecycle call) is cancelled midway (future dropped),
/// this VM has failed. It is not started or resumed again and not reported as running, and
/// runtime cleans it up. That instance ends in `Failed`, and a half-done VM never shows as
/// `Ready` (instance contract). `events` only subscribes, so dropping it has no side effects.
#[async_trait]
pub trait Machine: Send + Sync {
    /// Starts the VM.
    async fn start(&self) -> Result<()>;
    /// Pauses the VM.
    async fn pause(&self) -> Result<()>;
    /// Resumes the VM.
    async fn resume(&self) -> Result<()>;
    /// Writes a snapshot to `out`.
    async fn snapshot(&self, out: SnapshotSink) -> Result<SnapshotMeta>;
    /// Updates the VMGenID.
    async fn bump_generation(&self) -> Result<()>;
    /// Destroys the VM.
    async fn destroy(self: Box<Self>) -> Result<()>;
    /// VM event stream. Only subscribes and does not wait. Delivery happens on the stream side.
    fn events(&self) -> EventStream<VmEvent>;
}

/// Snapshot output target. PLACEHOLDER: structure undecided.
#[derive(Debug)]
pub struct SnapshotSink {}

/// VM event. PLACEHOLDER: value set undecided.
#[derive(Debug, Clone)]
pub struct VmEvent {}

/// Compile-only check: the async traits can be implemented by dummy types, can be used as
/// `Box<dyn …>` and `&dyn …`, and their futures are `Send`.
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use droiduse_contracts_common::{Error, ErrorCode};

    fn assert_send<T: Send>(_: T) {}
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

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

    fn unimplemented_err() -> Error {
        Error::new(ErrorCode::Unimplemented, "compile-only dummy")
    }

    struct DummyMemory;
    #[async_trait]
    impl MemorySource for DummyMemory {
        fn len(&self) -> u64 {
            0
        }
        async fn read_at(&self, _offset: u64, _len: usize) -> Result<Bytes> {
            Err(unimplemented_err())
        }
        async fn prefetch(&self, _ranges: &[std::ops::Range<u64>]) {}
    }

    struct DummyMachine;
    #[async_trait]
    impl Machine for DummyMachine {
        async fn start(&self) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn pause(&self) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn resume(&self) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn snapshot(&self, _out: SnapshotSink) -> Result<SnapshotMeta> {
            Err(unimplemented_err())
        }
        async fn bump_generation(&self) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn destroy(self: Box<Self>) -> Result<()> {
            Err(unimplemented_err())
        }
        fn events(&self) -> EventStream<VmEvent> {
            unimplemented!("compile-only dummy")
        }
    }

    struct DummyFactory;
    #[async_trait]
    impl MachineFactory for DummyFactory {
        async fn create(&self, _spec: VmSpec) -> Result<Box<dyn Machine>> {
            Ok(Box::new(DummyMachine))
        }
        async fn restore(
            &self,
            _spec: VmSpec,
            _meta: SnapshotMeta,
            _mem: Box<dyn MemorySource>,
        ) -> Result<Box<dyn Machine>> {
            Ok(Box::new(DummyMachine))
        }
    }

    /// Every future is `Send` even when called through a trait object.
    fn dyn_futures_are_send(f: &dyn MachineFactory, m: Box<dyn Machine>) {
        assert_send(f.create(spec()));
        assert_send(f.restore(spec(), SnapshotMeta {}, Box::new(DummyMemory)));
        assert_send(m.start());
        assert_send(m.pause());
        assert_send(m.resume());
        assert_send(m.snapshot(SnapshotSink {}));
        assert_send(m.bump_generation());
        assert_send(m.destroy());
    }

    #[test]
    fn dummy_implements_contract_as_dyn() {
        assert_send_sync::<dyn MachineFactory>();
        assert_send_sync::<dyn Machine>();
        let f: Box<dyn MachineFactory> = Box::new(DummyFactory);
        dyn_futures_are_send(f.as_ref(), Box::new(DummyMachine));
    }
}
