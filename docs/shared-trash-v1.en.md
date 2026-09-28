# Shared trash v1 — local 1.1.0 candidate

Android, Mac and Windows share the same protocol; iPhone implements it in test version 0.10.0. No public release has been published.

Only one cloud provider is connected. Before publishing a deletion, the client verifies an archive of metadata, transcript and existing cloud audio. Restore uses a stable new identity; retries do not create another recording. Original tombstones remain for older-client compatibility. Archives never expire automatically.

Archives use `meetings/trash-v1-<id>.json`, audio uses `audio/trash-v1-<id>.wav`, and restore markers use `meetings/trash-v1-<id>.restored.json`. Schema 1 contains id, deletion timestamp, raw metadata, transcript and optional audio MD5/size. JSON is capped at 20 MB. Restore ID is 4000000000000000 plus the first six big-endian SHA256 bytes of `solflow-trash-restore-v1:<id>`.

Old deletions without an archive can only recover cached local data. Extra local files remain in the retained original directory; they are not part of the cloud archive or restored recording. iPhone queues shared restores until cloud access is available; other clients can restore locally before uploading.

Validation and boundaries:

- The user authorized publication of test branch `work/shared-trash-google-ios`. Windows, Mac and Android CI builds passed at commit `0ef7aab`.
- Windows passed 41 application tests and 6 downloader checks. The installer and updater signature are verified. Its interface has not been manually tested.
- Mac passed 47 tests and signature verification with its existing certificate. Version 1.1.0 was installed over 1.0.0 with a full backup: all 155 library files matched before launch; 93 checked recording, project and model files remained unchanged after launch. All 18 existing recordings remained present.
- Android passed 4 trash tests and release APK signature verification with its existing key (versionCode 47). The connected Xiaomi was updated over 1.0.0 without uninstalling or clearing data.
- iPhone passed 72 tests; physical 0.10.0 validation includes Google login, listing, upload, download, transcript and audio playback, then reconnection to Yandex. Automatic cross-device Google sync has not been separately verified.

Physical Android–Mac trash acceptance is performed separately from build checks. No new public release or updater has been published as of this report. See [1.1.0 scope and limitations](release-1.1.0.en.md). Existing title-conflict fixes remain a separate work item.
