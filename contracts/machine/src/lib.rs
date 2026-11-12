//! machine 계약: runtime ↔ vmm.
//!
//! VM 하나의 정의와 제어. 상세: `docs/03-contracts.md` machine.
//!
//! **Android 개념 없음.** 이 crate는 `host/vmm`이 쓰므로 Android 개념을 다루는 식별자·문자열을
//! 두지 않는다 (`LAYERS.toml` `[forbidden]`). 게스트 bootconfig 값은 `host/runtime/instance`가
//! 만들어 불투명 key=value로 넘기고, vmm은 내용을 해석하지 않는다.
//!
//! VMM은 jail 안의 별도 프로세스다. runtime 쪽 구현은 `host/vmm/api`의 REST 클라이언트가 이 trait을
//! 구현한다 (in-process 구현은 테스트용). 그 확장 REST API의 스키마는 이 crate에 없다 (미정).
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use droiduse_contracts_common::{EventStream, Labels, Result};

/// `MachineFactory::restore`가 받는 스냅샷 메타데이터와 메모리 공급원.
///
/// 정의는 `spi` 계약에 있다 (`SnapshotStore`, `MemorySource` SPI와 같은 타입).
pub use droiduse_contracts_spi::{MemorySource, SnapshotMeta};

// ---------------------------------------------------------------------------
// VM 정의
// ---------------------------------------------------------------------------

/// VM 하나의 정의.
#[derive(Debug, Clone)]
pub struct VmSpec {
    /// vCPU 수.
    pub vcpus: u8,
    /// 게스트 메모리 (MiB).
    pub memory_mib: u32,
    /// CPU 템플릿. 스냅샷 이식성을 위해 필수.
    pub cpu_template: CpuTemplateRef,
    /// 부팅 방식.
    pub boot: BootSpec,
    /// 디스크. GPT + CoW 레이어(base ro / snapshot / profile).
    pub disks: Vec<CompositeDiskSpec>,
    /// vsock.
    pub vsock: VsockSpec,
    /// 네트워크. vhost-user-net 백엔드 소켓.
    pub net: Vec<NetSpec>,
    /// vhost-user 디바이스들. `{kind, socket, shm_regions}`.
    pub vhost_user: Vec<VhostUserDeviceSpec>,
    /// VMGenID 디바이스 사용 여부.
    pub vmgenid: bool,
    /// 불투명 map. 코어는 해석하지 않는다.
    pub labels: Labels,
}

/// 부팅 방식.
#[derive(Debug, Clone)]
pub enum BootSpec {
    /// 일반 Linux 부팅.
    Linux {
        /// 커널 이미지.
        kernel: PathBuf,
        /// initrd.
        initrd: PathBuf,
        /// 커널 cmdline.
        cmdline: String,
    },
    /// boot image v4 + vendor_boot **컨테이너 포맷** 부팅. 이름은 포맷 이름이다.
    BootImageV4 {
        /// boot 이미지.
        boot: PathBuf,
        /// vendor_boot 이미지.
        vendor_boot: PathBuf,
        /// bootconfig. 불투명 key=value 목록. vmm은 내용을 해석하지 않는다.
        /// 순서·중복 키·값 이스케이프 규칙은 미정이다.
        bootconfig: Vec<(String, String)>,
    },
}

/// CPU 템플릿 참조 (게스트에 보이는 CPU 기능 고정).
///
/// PLACEHOLDER: 표현 미정 (문자열은 "불투명 값"이라는 뜻일 뿐).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CpuTemplateRef(pub String);

/// GPT 컴포지트 디스크 + CoW 레이어(base ro / snapshot / profile).
///
/// PLACEHOLDER: 구조 미정. 이름 있는 파티션 구성은 `docs/05-vmm-and-devices.md` §3 `host/vmm/disk`.
#[derive(Debug, Clone)]
pub struct CompositeDiskSpec {}

/// vsock 설정. PLACEHOLDER: 구조 미정.
#[derive(Debug, Clone)]
pub struct VsockSpec {}

/// 네트워크 하나 (vhost-user-net 백엔드 소켓). PLACEHOLDER: 구조 미정.
#[derive(Debug, Clone)]
pub struct NetSpec {}

/// vhost-user 디바이스 하나. 문서: `{kind, socket, shm_regions}`.
#[derive(Debug, Clone)]
pub struct VhostUserDeviceSpec {
    /// 디바이스 종류.
    pub kind: VhostUserDeviceKind,
    /// vhost-user 백엔드의 unix socket 경로.
    pub socket: PathBuf,
    /// virtio shared memory region들 (gfxstream blob resource 등).
    pub shm_regions: Vec<ShmRegionSpec>,
}

/// vhost-user 디바이스 종류.
///
/// PLACEHOLDER: 문서에 필드 이름(`kind`)만 있다. 값 집합·표현 미정.
#[derive(Debug, Clone)]
pub struct VhostUserDeviceKind {}

/// virtio shared memory region 하나.
///
/// PLACEHOLDER: 문서에 필드 이름(`shm_regions`)만 있다. 구조 미정.
#[derive(Debug, Clone)]
pub struct ShmRegionSpec {}

// ---------------------------------------------------------------------------
// 제어
// ---------------------------------------------------------------------------

/// VM 생성 경로. `Machine`과 분리해 둘 다 trait object(dyn)로 쓸 수 있게 한다.
pub trait MachineFactory {
    /// 새 VM을 만든다.
    fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>>;
    /// 스냅샷에서 VM을 복원한다.
    fn restore(
        &self,
        spec: VmSpec,
        meta: SnapshotMeta,
        mem: Box<dyn MemorySource>,
    ) -> Result<Box<dyn Machine>>;
}

/// VM 하나의 제어.
pub trait Machine {
    /// 시작한다.
    fn start(&self) -> Result<()>;
    /// 일시정지한다.
    fn pause(&self) -> Result<()>;
    /// 재개한다.
    fn resume(&self) -> Result<()>;
    /// 스냅샷을 `out`으로 쓴다.
    fn snapshot(&self, out: SnapshotSink) -> Result<SnapshotMeta>;
    /// VMGenID를 갱신한다.
    fn bump_generation(&self) -> Result<()>;
    /// VM을 없앤다.
    fn destroy(self: Box<Self>) -> Result<()>;
    /// VM 이벤트 스트림.
    fn events(&self) -> EventStream<VmEvent>;
}

/// 스냅샷 출력 대상. PLACEHOLDER: 구조 미정.
#[derive(Debug)]
pub struct SnapshotSink {}

/// VM 이벤트. PLACEHOLDER: 값 집합 미정.
#[derive(Debug, Clone)]
pub struct VmEvent {}
