//! device contract: vmm ↔ devices.
//!
//! Details: `docs/03-contracts.md` device, `docs/05-vmm-and-devices.md` §4.
//!
//! The base is the **vhost-user standard protocol** (unix socket + shm). Devices are
//! implemented as separate vhost-user backend processes. The standard messages are defined in
//! the vhost-user specification, and this crate does not define them again. This crate holds
//! only the Rust-side types of what the documentation lists as "extensions" and we have to
//! define.
//!
//! The documentation's list of extensions and the scope of this crate:
//! - device state transfer (the `SET_DEVICE_STATE_FD` family): messages of the vhost-user
//!   specification. No types for it are defined here.
//! - virtio shared memory regions (for gfxstream blob resources): a concept of the virtio
//!   specification. The VM-side setting is `VhostUserDeviceSpec::shm_regions` of the `machine`
//!   contract. No types for it are defined here.
//! - **scanout export** (gpu only): our extension. The types below.
//!
//! **Knows nothing of Android.** `host/vmm` and `host/devices` use this crate, so no identifier
//! or string that handles an Android concept goes here (`LAYERS.toml` `[forbidden]`). The guest
//! frameId is not put into the frame metadata. The guest frameId comes over guest_link, and the
//! `FrameChannel` of `host/runtime/instance` matches the two **exactly** by the virtio-level
//! identifier (`ScanoutRef`). There is no approximate matching.
//!
//! The wire format of these types (over which channel, in what byte layout) is undecided.
//!
//! A type marked `PLACEHOLDER` is one whose concept only the documentation fixed. Its
//! structure and meaning are undecided.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The virtio-level identifier of a scanout frame.
///
/// It is the same value as `scanout_ref` of guest_link `Frame.Committed`, so it is the key of
/// the exact match.
///
/// PLACEHOLDER: which virtio-level values it consists of, and how equality is defined, is
/// undecided. Exact matching depends on this type's equality, so it gets no `PartialEq` until
/// its structure is fixed.
#[derive(Debug, Clone)]
pub struct ScanoutRef {}

/// Shared memory handle (scanout buffer).
///
/// PLACEHOLDER: per-platform representation undecided. The documentation says "shm fd", but a
/// Windows host has no fd passing (HANDLE). Ownership (who closes it) is undecided too.
#[derive(Debug)]
pub struct ShmHandle {}

/// scanout export: the current scanout buffer.
#[derive(Debug)]
pub struct ScanoutBuffer {
    /// Shared memory of the current scanout buffer.
    pub shm: ShmHandle,
}

/// scanout export: frame commit notification.
#[derive(Debug, Clone)]
pub struct ScanoutCommit {
    /// The virtio-level identifier of the committed frame.
    pub scanout_ref: ScanoutRef,
}
