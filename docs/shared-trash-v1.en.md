# Shared trash v1 — local 1.1.0 candidate

Android, Mac and Windows share the same protocol; iPhone implements it in test version 0.10.0. Nothing has been published.

Only one cloud provider is connected. Before publishing a deletion, the client verifies an archive of metadata, transcript and existing cloud audio. Restore uses a stable new identity; retries do not create another recording. Original tombstones remain for older-client compatibility. Archives never expire automatically.

Archives use `meetings/trash-v1-<id>.json`, audio uses `audio/trash-v1-<id>.wav`, and restore markers use `meetings/trash-v1-<id>.restored.json`. Schema 1 contains id, deletion timestamp, raw metadata, transcript and optional audio MD5/size. JSON is capped at 20 MB. Restore ID is 4000000000000000 plus the first six big-endian SHA256 bytes of `solflow-trash-restore-v1:<id>`.

Old deletions without an archive can only recover cached local data. Extra local files remain in the retained original directory; they are not part of the cloud archive or restored recording. iPhone queues shared restores until cloud access is available; other clients can restore locally before uploading.

Validation: 47 Rust tests, 4 Android tests, full Mac debug build, Android debug APK, and 72 iPhone tests passed. iPhone 0.10.0 is installed on the physical test device with the library preserved. The user confirmed Google sign-in and successful Drive listing on the physical iPhone; the user also confirmed recording upload, download, text and audio playback. The manual Google round trip passed; automatic cross-device Google sync remains untested. Native Windows builds and cross-device trash acceptance are pending. Existing title-conflict fixes remain a separate work item.

See the Russian companion document for the protocol and exact verification boundaries. No release, push, or production desktop/Android installation was performed.

The user restored iPhone Yandex sync successfully. Stale archive audio cache recovery is fixed without changing the retained local original. An optimized Mac binary and app candidate are prepared, but signing awaits unlocking the existing solflow-build keychain. Mac 1.0.0 is unchanged. Rust/Android tests were added to the existing CI workflows.
