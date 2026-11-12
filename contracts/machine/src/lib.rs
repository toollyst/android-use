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
//! 비동기 메서드는 `async-trait`으로 선언한다. trait object(dyn)로 쓸 수 있고, 특정 async
//! 런타임을 가정하지 않으며, `Send` future를 돌려준다. future를 drop하면 그 호출은 취소된다.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use async_trait::async_trait;
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
///
/// **취소**: `create`·`restore`가 도중에 취소되면(future drop) 만들던 VM은 실패한 것이다.
/// 호출자에게 VM이 돌아가지 않고, 반쯤 만든 VMM 프로세스와 자원의 정리는 runtime이 맡는다.
/// 그 인스턴스는 `Failed`로 끝난다 (instance 계약).
#[async_trait]
pub trait MachineFactory: Send + Sync {
    /// 새 VM을 만든다.
    async fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>>;
    /// 스냅샷에서 VM을 복원한다.
    async fn restore(
        &self,
        spec: VmSpec,
        meta: SnapshotMeta,
        mem: Box<dyn MemorySource>,
    ) -> Result<Box<dyn Machine>>;
}

/// VM 하나의 제어.
///
/// **취소**: 비동기 메서드(수명주기 호출)가 도중에 취소되면(future drop) 이 VM은 실패한 것이다.
/// 다시 시작·재개하지 않고 실행 중으로 보고하지 않으며, 정리는 runtime이 맡는다.
/// 그 인스턴스는 `Failed`로 끝나고 반쯤 된 VM이 `Ready`로 보이는 일은 없다 (instance 계약).
/// `events`는 구독만 하므로 drop에 부작용이 없다.
#[async_trait]
pub trait Machine: Send + Sync {
    /// 시작한다.
    async fn start(&self) -> Result<()>;
    /// 일시정지한다.
    async fn pause(&self) -> Result<()>;
    /// 재개한다.
    async fn resume(&self) -> Result<()>;
    /// 스냅샷을 `out`으로 쓴다.
    async fn snapshot(&self, out: SnapshotSink) -> Result<SnapshotMeta>;
    /// VMGenID를 갱신한다.
    async fn bump_generation(&self) -> Result<()>;
    /// VM을 없앤다.
    async fn destroy(self: Box<Self>) -> Result<()>;
    /// VM 이벤트 스트림. 구독만 하고 기다리지 않는다. 전달은 스트림 쪽에서 일어난다.
    fn events(&self) -> EventStream<VmEvent>;
}

/// 스냅샷 출력 대상. PLACEHOLDER: 구조 미정.
#[derive(Debug)]
pub struct SnapshotSink {}

/// VM 이벤트. PLACEHOLDER: 값 집합 미정.
#[derive(Debug, Clone)]
pub struct VmEvent {}

/// 컴파일 전용 검사: 비동기 trait을 더미 타입으로 구현할 수 있고, `Box<dyn …>`·`&dyn …`로 쓸 수
/// 있으며, future가 `Send`다.
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use droiduse_contracts_common::Error;

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
        Error {
            code: None,
            message: String::new(),
        }
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
            unimplemented!("컴파일 전용 더미")
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

    /// trait object로 호출해도 모든 future가 `Send`다.
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
