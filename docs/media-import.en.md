# Media import: version 0.9.8

[Русский](media-import.md)

## Changes

- Mac tries the system audio converter first, then falls back to FFmpeg for unsupported containers/codecs. This handles MPEG-TS/HLS, including Rutube streams that may use an `.mp4` extension.
- The downloader prefers an audio-only track. When unavailable, it selects a smaller video rendition because transcription does not need the picture.
- Metadata and media use a single process. Progress processing no longer blocks the downloader, and stderr is drained concurrently. Cancellation stops the process tree; incomplete files cannot become successful recordings.
- Downloaded source media is retained if conversion fails, even when source retention is disabled. Successful imports follow the regular retention setting.
- Settings can install or update download components. On Mac and Windows, Sol Flow manages yt-dlp, FFmpeg and Deno in the application data `bin` directory. Downloads are checksum-verified and the new executable is tested before activation; a corrupt download does not replace the working binary.

## YouTube

Public videos often download without an account. “Confirm you are not a bot” can depend on the network/IP, VPN, video restrictions and YouTube checks. Signing in through a browser does not automatically share that session with Sol Flow.

First update the downloader in Settings and retry. If authentication is needed, select the browser where the video plays, using the session selector beside the YouTube import. “No account” is the default. Selection applies to one download and then resets; other sites cannot use it. Sol Flow passes only the selected browser name to yt-dlp and does not save cookies with recordings or sync them. The OS or browser may prevent session access. Authentication cannot guarantee that YouTube restrictions are removed.

[Official yt-dlp cookies FAQ](https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp)

## Platforms

These importer changes apply to Mac and Windows. Android retains local-file, direct-media-link and Yandex Disk imports; YouTube/Rutube page downloading has not been added to the phone. Completed recordings can use normal synchronization. Android version/history are aligned with 0.9.8; Windows still needs testing on a real computer.

## Components and provenance

- yt-dlp and Deno: official GitHub Releases, verified against release SHA-256 metadata. Standalone yt-dlp includes EJS components; Deno is supplied through an explicit executable path.
- Windows FFmpeg: `yt-dlp/FFmpeg-Builds`, verified against the published archive SHA-256.
- Apple Silicon FFmpeg 7.1: `imageio/imageio-binaries`, commit `f8f64710ea88e7e4a352c0f7d8c0deac9f5fd685`, file `ffmpeg/ffmpeg-macos-aarch64-v7.1`, SHA-256 `6d175a4743ca50256e89a8cdd731100f9cee33bd79aeea46894d209410dc6617`.

Executables are downloaded separately when installing components and are not part of the app source tree. Their licenses continue to apply: [yt-dlp](https://github.com/yt-dlp/yt-dlp#license), [Deno](https://github.com/denoland/deno/blob/main/LICENSE.md), [FFmpeg](https://ffmpeg.org/legal.html), [ImageIO binaries](https://github.com/imageio/imageio-binaries/tree/f8f64710ea88e7e4a352c0f7d8c0deac9f5fd685/ffmpeg).

## Verification

`scripts/check-media-import.py` compiles the actual downloader modules and tests error handling, partial files, progress and cancellation. Set `SOLFLOW_TEST_DEPS` to a coherent Rust build's `release/deps` directory. `--install DIR` and `--fetch DIR URL` operate in an isolated folder and do not use browser sessions. `scripts/check-workbench-ui.cjs` checks the interface with isolated Russian and English fixtures.

See [release 0.9.8](release-0.9.8.en.md) for validation and release scope.
