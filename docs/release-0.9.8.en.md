# Sol Flow 0.9.8

[Русский](release-0.9.8.md) · **English**

Fixes for Rutube and additional media formats, more reliable YouTube importing and download stalls.

## Changes since 0.9.7

- **Rutube and MPEG-TS on Mac.** If the system converter cannot read a stream or container, the app falls back to FFmpeg. A file ending in `.mp4` is no longer assumed to contain a conventional MP4 container. Direct `.ts`, `.mts` and `.m2ts` links are supported.
- **Fewer download delays.** Metadata and media use one process. Progress handling no longer blocks the downloader; error output is drained concurrently so a full pipe cannot stall the download. Cancellation terminates child processes, and incomplete files are never treated as finished recordings.
- **Less unnecessary data.** Audio-only tracks are preferred; when unavailable, a smaller video rendition is selected for audio extraction.
- **Component updates.** Mac and Windows Settings now retain an “Update downloader” button. Sol Flow manages its own yt-dlp, Deno and FFmpeg, verifying checksums and executable startup before replacing a working component.
- **Explicit browser selection for YouTube.** Imports use no account by default. If YouTube requires sign-in, select a browser where the video is accessible. Selection applies to one attempt and resets afterwards; cookies are not saved with recordings or synced. Browser sessions are not read automatically.
- **Clear errors and retries.** Messages distinguish YouTube checks, access restrictions, network failures and session-reading problems. The URL remains available for retry. Downloaded source media is preserved if conversion fails.
- **Two languages.** The new interface, messages and instructions are available in Russian and English.

## Download and update

- [Windows 10/11 x64](https://github.com/isoloma-ux/solflow/releases/download/v0.9.8/SolFlow_0.9.8_x64-setup.exe).
- [Mac with Apple silicon, macOS 14+](https://github.com/isoloma-ux/solflow/releases/download/v0.9.8/SolFlow_0.9.8_macOS.zip).
- [Android 8+ arm64](https://github.com/isoloma-ux/solflow/releases/download/v0.9.8/SolFlow_0.9.8.apk).

On desktop, click the version number at the bottom of the window and confirm the offered update. Alternatively, install the file above over the existing version; on Mac move the app from the ZIP into Applications, and on Android install the APK over the app. There is no need to uninstall first; application identifiers and signing keys are retained.

**After updating, open Settings → Link downloader → Install / update.** Existing downloader installations also need this update to add the YouTube components. Then retry the affected link.

## Platforms and limitations

Downloader changes apply to Mac and Windows. Android receives the version number and change history update; YouTube/Rutube page downloading has not been added to the phone. Android continues to support local files, direct media links and Yandex Disk.

YouTube may restrict access based on the network/IP, account or video conditions. Signing in through a browser does not automatically share the session with Sol Flow, and explicitly using it cannot guarantee access. If the browser or OS prevents session reading, the app reports an error. [Detailed instructions](media-import.en.md).

Windows does not yet have Authenticode signing. Mac uses the existing Sol Flow certificate without Apple notarization; first-time installations may show system warnings. macOS 12–13 are not supported.

## Release validation

Mac validation covered launching 0.9.8 with existing data, downloading an entire approximately 1 h 55 min Rutube film, converting all of its audio and locally transcribing a 40-second sample. A test YouTube video downloaded completely without an account. All 11 downloader regression tests and Russian/English interface checks in both themes passed.

Publication is gated on successful Mac, Windows and Android builds, a Windows component-installation check, APK and desktop updater signature verification and a complete updater manifest. Manual 0.9.8 runtime testing on Windows and Android has not been performed; the tester's YouTube block has not been reproduced on their computer. Full-film transcription was not run during validation.

## App features

Dictation into other apps, meeting recording with pause, audio/video import, timestamped transcripts, search, projects, playback and exports. Speakers can be reviewed, assigned manually and renamed. Mac and Windows locally generate summaries, titles, translations, answers about a recording and topic-based analyses: decisions, tasks, follow-up letters, outlines and other formats.

Recording maps are generated locally on desktop. Mac, Windows and Android support viewing, editing and PNG/SVG export with the Sol Flow logo. Sync uses your own Yandex.Disk or Google Drive; audio transfer is optional. Android does not run the large language model but can receive finished summaries, titles and maps through sync.

[Full feature overview](../README.en.md#what-it-does) · [Changes in 0.9.7](release-0.9.7.en.md).
