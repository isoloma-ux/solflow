# Sol Flow cross-platform pilot — 26 September 2026

Test build 0.9.9 with MCP and the new guide; not a public release. Android and Windows require user acceptance before release.

## Verified

- Signed release APK installed over the existing app on Xiaomi 24129PN74G. Signing certificate matches; no uninstall or data reset. App and GigaAM launch.
- A 17.85-second synthetic WAV was imported into Test MCP and transcribed on Android. It includes the marker “сиреневый маяк”. Recognition of the product name has errors.
- Android → existing cloud account → Mac: project, recording, transcript and duration arrived. The real local MCP process returns the transcript and generated summary.
- Mac → cloud → Android: generated title and summary arrived and opened on the phone. The other cloud provider was not tested separately.
- Claude Desktop normal Chat, synthetic `solflow-pilot` only: project/recording lists, search, fetch of two passages with timestamps. Live revocation returned a denied fetch and empty project list without a restart. The original synthetic grant was restored. Real interviews were not sent to Claude.
- Seven MCP tests passed, including actual SDK process exchange, live revocation, isolation and preservation of source files.
- Android assembleRelease and Mac cargo build passed. Signatures checked; Mac installed using the previous identity with application/data backups.

## Guide

First launch and About → Sol Flow guide. Eleven RU/EN sections, platform-specific notes, screenshots with sample data, user-triggered step animation, reduced-motion support. Bundled resources, no network or user-data access. Browser navigation, language/platform selection, screenshots and animation checked; Android WebView checked on the phone; guide checked in the installed Mac app. A clean install was not performed to preserve user data.

## Remaining release gates

- Windows NSIS build and real-device update, speech/LLM, MCP sidecar, guide, deep links and bidirectional sync. No EXE has been produced yet.
- Android dictation into another app, long/pause recordings, format imports/exports, audio playback and audio sync.
- Full map round trip Mac → Android edit → desktop and PNG/SVG export.
- Concurrent-edit conflicts and second cloud provider.
- Recheck Android Accessibility after an update: the phone displayed a service-disabled notification. System permissions were not changed automatically.
- Choose release version, update both release histories, validate final package signatures/hashes/updater and obtain publication approval.

## Claude setup

Enable a project and selected material types in desktop Sol Flow. Open AI connection setup and choose Claude Desktop. Merge the generated entry into `mcpServers` through Claude Settings → Developer → Edit Config, preserving the existing configuration and a backup. Fully restart Claude and use a normal Chat. Request projects, search a known phrase and read the source with a timestamp; allow the tool as needed. `fetch` requires a document id returned by search (for example `100:transcript:0:0`), not recording id `100`. Search is lexical rather than semantic. Revoke access and test a fresh tool call; prior chat text remains. Current `solflow-pilot` uses a separate synthetic export, not real interviews. Browser/mobile clients do not inherit this local configuration.

## Windows source upload

Origin push URL is `/dev/null`. The prepared `build` workflow produces a Windows installer artifact from a test branch without a release tag. Source upload to GitHub needs confirmation before overriding that local guard. Do not push a tag or run release.yml for this pilot.
