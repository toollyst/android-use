# AOSP hook 패치

`$AOSP_ROOT`의 프로젝트 경로별로 둔다. 적용 순서는 `series` (`<project-path> <patch-file>`).
헤더 템플릿: `docs/08-upstream-and-patches.md`

## 패치

패치 헤더에서 생성한 표다 (`cargo xtask patch-readme`). 손으로 고치지 않는다.

<!-- BEGIN GENERATED: patch table from the patch headers. Run `cargo xtask patch-readme`. -->
_No patches._
<!-- END GENERATED: patch table -->

## 계획 (아직 패치가 아니다)

| 프로젝트 | 패치 | Hook-for | 단계 |
|---|---|---|---|
| frameworks/base | 0001-droiduse-viewroot-inspector-hook | guest/vendor/services/inspector | M2 |
| frameworks/base | 0002-droiduse-wm-activity-event-hook | guest/vendor/services/inspector | M2 |
| frameworks/base | 0003-droiduse-webview-debug-policy | guest/vendor/services/inspector | M4 |
| frameworks/native | 0001-droiduse-sf-frameid-layer-capture | guest/vendor/services/adpd | M2 |
| frameworks/libs/binary_translation | berberis 프런트엔드 (신규 파일 / 수정 분리) | guest/vendor/nativebridge | v1 이후 |

정확한 삽입 지점은 고정 태그 기준으로 착수 시 확정한다.
