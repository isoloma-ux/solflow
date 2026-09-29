# Sol Flow 1.1.1 — local validation

## Passed on 2026-09-29

- Android debug and optimized release builds; 12 JVM unit tests, including every prefix of Chinese/Korean and localized/English language names.
- Android emulator: K, Ko, Kor, Korean, C, Chinese retain matching results. Real drawer and Settings screens checked on all six new interface languages.
- Desktop Rust: `cargo check --locked --no-default-features`; 53 library tests passed.
- Desktop browser fixture: all six interface languages rendered; Settings fit at 1280px without horizontal overflow. Language search K→Korean and C→Chinese passed. Japanese Android guide loads translated content and sample screenshots.
- Dictionary generation: 1742 English UI keys in each of six languages; complete desktop key coverage, placeholder preservation, source paths/URLs and generated resource freshness.
- MCP configuration helper: existing 18 setup-prompt tests passed. Project sharing and canonical synchronization data were not changed by localization.
- Separate iPhone test workspace: simulator build and InterfaceLanguageTests succeeded. This is not an App Store release.

## Boundaries

- Physical Android/iPhone and a physical Windows PC have not been tested with 1.1.1 yet.
- Local Android packages use the existing debug certificate. Public update packages must be produced with the release signing key by CI; local packages are not evidence of official update compatibility.
- Desktop browser fixtures verify layout and interaction, not native Windows installation.
- Translations were reviewed by AI translation agents; professional native-speaker review has not been completed.
- CI, final artifact signatures, public assets and updater metadata must still be verified before publication.
