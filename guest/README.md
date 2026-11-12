# guest/ (L3)

Guest Android. Details: `docs/06-guest-aosp.md`

- `vendor/`: the AOSP overlay. `tools/sync/aosp-sync.sh` puts it at `$AOSP_ROOT/vendor/droiduse` by **rsync or bind mount** (no symlinks: a Soong constraint).
- `kernel/`: `fragments/` (config fragments), `modules/` (vendor modules when needed, KMI compliant)

```
vendor/
├─ AndroidProducts.mk
├─ products/{wick_arm64,wick_x86_64,common}/
├─ services/{inspector,adpd,agent,restore}/
├─ hals/{graphics,keymint,health,sensors,audio}/
├─ nativebridge/  apps/microg/  sepolicy/  avb/  protocol/
```

Rules: the release build is a user build, AVB on, SELinux enforcing. Module visibility is restricted with Soong `visibility`.
