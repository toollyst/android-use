# AOSP hook patches

Kept per project path in `$AOSP_ROOT`. The application order is `series` (`<project-path> <patch-file>`).
Header template: `docs/08-upstream-and-patches.md`

## Patches

This table is generated from the patch headers (`cargo xtask patch-readme`). Do not edit it by hand.

<!-- BEGIN GENERATED: patch table from the patch headers. Run `cargo xtask patch-readme`. -->
_No patches._
<!-- END GENERATED: patch table -->

## Plan (not patches yet)

| Project | Patch | Hook-for | Stage |
|---|---|---|---|
| frameworks/base | 0001-droiduse-viewroot-inspector-hook | guest/vendor/services/inspector | M2 |
| frameworks/base | 0002-droiduse-wm-activity-event-hook | guest/vendor/services/inspector | M2 |
| frameworks/base | 0003-droiduse-webview-debug-policy | guest/vendor/services/inspector | M4 |
| frameworks/native | 0001-droiduse-sf-frameid-layer-capture | guest/vendor/services/adpd | M2 |
| frameworks/libs/binary_translation | berberis frontend (new files / modifications kept separate) | guest/vendor/nativebridge | after v1 |

The exact insertion points are fixed against the pinned tag when the work starts.
