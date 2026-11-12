# guest/ (L3)

게스트 Android. 상세: `docs/06-guest-aosp.md`

- `vendor/`: AOSP overlay. `tools/sync/aosp-sync.sh`가 `$AOSP_ROOT/vendor/droiduse`로 **rsync 또는 bind mount**한다 (symlink 금지: Soong 제약).
- `kernel/`: `fragments/`(config fragment), `modules/`(필요 시 vendor 모듈, KMI 준수)

```
vendor/
├─ AndroidProducts.mk
├─ products/{wick_arm64,wick_x86_64,common}/
├─ services/{inspector,adpd,agent,restore}/
├─ hals/{graphics,keymint,health,sensors,audio}/
├─ nativebridge/  apps/microg/  sepolicy/  avb/  protocol/
```

규칙: 배포 빌드는 user 빌드, AVB on, SELinux enforcing. 모듈 가시성은 Soong `visibility`로 제한한다.
