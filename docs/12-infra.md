# 12. Build and test infrastructure

## 1. Machines
| Machine | Role |
|---|---|
| **GCE build server** (n2-standard-96 **Spot**, Seoul + Local SSD) | AOSP, kernel and host builds, CTS shards. `out/` on Local SSD, source cache on a persistent disk, ccache. Turned on only to build |
| **GCE C4A bare metal** (e.g. `c4a-highmem-96-metal`, Axion arm64) | (Tokyo) Linux arm64 (native KVM) **gate verification** and density and performance metrics. **Turned on only for gate verification** |
| **GCE x86** (Intel VM nested or bare metal) | x86_64 guest boot tests; performance metrics on bare metal |
| **Local Windows PC → Linux dual boot** (a separate SSD recommended) | **Verifying the gfxstream host GPU path (Linux)** with a native GPU driver + bare metal KVM. Covers the part where the GPU+KVM combination is hard on GCE |
| **Windows** on the same PC | Native Windows verification machine once wick boots on Windows (WHP) for the first time |
| **Mac (Apple silicon, macOS 15+)** | **Everyday arm64 guest boot and test iterations (HVF)**, Metal (MoltenVK), seatbelt, Secure Enclave, gate and v1 release verification. GCE has no Mac instances |

## 2. Caveats
- **WSL2 is auxiliary only**: KVM works nested, but the GPU goes through DirectX paravirtualization + a Vulkan-on-D3D12 translation layer, which makes it unsuitable for gfxstream verification. It is not one of the production targets.
- **The GPU + KVM combination on GCE** has to be checked per machine series. If it does not work, the GPU path is verified on the local Linux PC and the Mac.
- **Cost**: the build server is Spot and on only while building; C4A bare metal is on only for gate verification. CI brings machines up when needed and takes them down when done. CTS shards run on Spot VMs. The cost baseline is `docs/13-roadmap.md` §4.

## 3. Infrastructure needed for M0
1. Create the build server → start `repo sync` of the AOSP release tag (takes hours; first of all)
2. wick Linux build + boot test harness on C4A bare metal (`tools/infra/boot-test.sh`)
3. Image cache bucket and CI runner registration
4. (for M5) Local PC Linux dual boot + GPU driver + KVM check

## 4. tools/infra
```
tools/infra/
├─ gce/            instance create/delete scripts (build server, C4A, x86)
├─ boot-test.sh    image → wick boot → boot_completed → check guest_link Hello
├─ cts.sh          run CTS shards and collect results
└─ runners/        self-hosted CI runner configuration
```
