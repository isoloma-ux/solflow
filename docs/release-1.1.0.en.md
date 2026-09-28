# Sol Flow 1.1.0

[Русский](release-1.1.0.md)

Shared recording trash for Android, Mac and Windows. Devices use one selected provider — Yandex Disk or Google Drive — and the same account.

## Changes

- Deleted recordings appear in Trash. Before sharing a deletion, the app saves and verifies an archive of the transcript, metadata and available audio.
- Restore brings back the title, project, transcript, summary and map. Audio is restored when it remains available in the local or cloud archive.
- Retrying an interrupted restore uses the same new identity to avoid creating another copy. The original deletion marker does not affect the restored recording.
- Stale local audio is checked against the archive and downloaded again when needed. A verification failure does not replace the retained original.
- Trash is available in the desktop and Android side menus. The interface and update notes are available in Russian and English.

## Test the feature

Update the devices and keep the same cloud provider and account selected. Create a separate short test recording, let it sync, delete it on one device and restore it from Trash on the other. After syncing, check the transcript, project and playback. Repeated syncing must not duplicate or delete the restored recording.

## Limitations

Archives never expire automatically and consume space in the selected cloud. Older deletions without an archive cannot be restored from the cloud. Extra local files remain in the retained original directory but are not included in the shared archive or automatically copied into the restored recording. Shared trash sync requires connectivity.

This release does not change title conflict resolution across devices: the previously observed title rollback remains a separate work item. iPhone uses a separate test build and is not included in this public release.

## Updating and validation

Packages: `SolFlow_1.1.0_x64-setup.exe`, `SolFlow_1.1.0_macOS.zip`, `SolFlow_1.1.0.apk` (Android versionCode 47). Install over the previous version without uninstalling the app or clearing its data.

The Windows test package passed compilation, 41 application tests, 6 downloader checks and updater signature verification. Mac passed 47 tests and signature verification with the existing app identity. Android passed trash tests and verification of the existing signing key. The Windows interface was not manually tested for this release; this is an accepted validation limit, not a guarantee that no bugs remain.

Mac uses the existing self-signed certificate without Apple notarization. Windows has no Authenticode signature; updater package signatures are checked separately. Distribution uses GitHub and the built-in updater, not app stores.

Technical format: [shared trash v1](shared-trash-v1.en.md).
