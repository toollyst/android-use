# AOSP hook 패치

`$AOSP_ROOT`의 프로젝트 경로별로 둔다. 적용 순서는 `series` (`<project-path> <patch-file>`).

| 프로젝트 | 패치 | Hook-for | 단계 |
|---|---|---|---|
| frameworks/base | 0001-droiduse-viewroot-inspector-hook | guest/vendor/services/inspector | 1M |
| frameworks/base | 0002-droiduse-wm-activity-event-hook | guest/vendor/services/inspector | 1M |
| frameworks/base | 0003-droiduse-webview-debug-policy | guest/vendor/services/inspector | v2 |
| frameworks/native | 0001-droiduse-sf-frameid-layer-capture | guest/vendor/services/adpd | 1M |
| frameworks/libs/binary_translation | berberis 프런트엔드 (신규 파일 / 수정 분리) | guest/vendor/nativebridge | v3 |

정확한 삽입 지점은 고정 태그 기준으로 착수 시 확정한다. 헤더 템플릿: `docs/08-upstream-and-patches.md`
