# Sol Flow 1.0 roadmap and outstanding work

**26.09.2026 — Android MCP:** Android: purple project markers, AI access controls and permission sync implemented and verified on Xiaomi and Mac. Android guide uses actual mobile screenshots; the old Show steps animation is removed. [Details](mcp-android-access.md).


Current cross-platform checks and remaining release gates: [pilot report](pilot-cross-platform-20260926.en.md).

Update, 2026-09-26: the first pilot now targets the current ChatGPT/Codex app. The fictional server is registered; direct in-chat calls, search, private-project isolation and live revocation passed. The signed local Mac build is installed with backups. Native transcript permissions, real-project search/fetch and live revocation passed for “Тест MCP”. Server `solflow` is registered; direct in-chat calls await client tool refresh. [Pilot status](mcp-openai-pilot.en.md).

Updated September 26, 2026. `work/1.0-mcp` starts at refreshed `origin/main`, commit `1c74098821765962f1a3d89ee07d4429c9efa0cb`. The app version remains 0.9.9 during local development. The local Mac pilot is installed; nothing has been published. The untracked AGENTS.md is not added automatically.

## 1. MCP now

Implemented locally: a separate Rust stdio server using official RMCP 3.4.1; five read-only tools; a managed SQLite FTS5 export; opt-in permissions per project and material type; per-call access checks and live revocation; bilingual desktop controls; explicit Markdown copies; importing files/links into the selected project on desktop and Android; desktop source-link handling in code; synthetic preservation and protocol tests.

Still required: full native Mac testing (including cold/warm source links and concurrent sync), Windows MCP packaging/signing and clean-system installation (the current Mac installation passed), a real Claude Desktop pilot with synthetic meetings, Windows build/manual verification, broader Russian search quality testing, direct in-chat verification of the registered real-project server after client tool refresh, and a separate connection for ordinary chatgpt.com. The current Codex client already passed direct fictional-tool calls over stdio. Inbox watching, duplicate prevention, .mcpb installation, and an in-app audit viewer remain later work. Saving AI results requires a separate preview/confirmation flow and must not overwrite originals.

The managed MCP source is a transactional database. Markdown copies are created explicitly and remain ordinary files after permission revocation. See [local MCP implementation](mcp-local.en.md) and the [original design](mcp-projects-plan.md).

## 2. Developer accounts immediately after MCP

This replaces iPhone development as the next phase, as requested September 26.

- Establish individual/organization ownership, actual country, existing Apple/Google accounts and available payment method.
- Complete Apple Developer Program and Google Play Console enrollment, identity checks and payment together.
- Set up Apple signing, TestFlight and Mac Developer ID/notarization; membership alone does not publish an app or validate builds.
- Prepare Play signing/AAB, app information, privacy policy and required testing.
- Decide separately whether to migrate Android `com.handy.voice`; preserve existing installations and data. Do not change it automatically.
- Confirm current terms, regional price and final payment during enrollment. No purchase or agreement acceptance has happened in this iteration.

Official conditions checked September 26: [Apple, USD 99/year with regional pricing](https://developer.apple.com/programs/enroll/); [Google Play, USD 25 once](https://support.google.com/googleplay/android-developer/answer/6112435); [testing requirements for new personal Play accounts](https://support.google.com/googleplay/android-developer/answer/14151465). Verify registration/payment availability for the user's actual country.

## 3. Transcription quality backlog

Source: `11_Sol_Flow/Качество_расшифровки_заметки_2026-09-23.md`. These are observations and hypotheses, not validated fixes.

- Evaluate speaker boundaries near acoustic pauses without changing attribution merely for readability.
- Distinguish actual overlap from speaker changes using labeled examples before choosing thresholds.
- Review empty/punctuation-only segments and boundary artifacts while keeping originals.
- Project dictionaries for names/brands and mixed-alphabet highlighting; keep replacements reviewable.
- Regression fixtures for numbers, money and time; preserve original wording when normalization is uncertain.
- Optional clean-reading mode alongside verbatim text.
- Readable paragraphs and less frequent displayed timestamps while retaining precise source navigation.

## 4. Remaining work

Physical Huawei ELS-N39 safe-area/overlay testing; manual Windows UX verification; Windows signing/SmartScreen as a separate decision; return to the preserved iPhone PoC only after MCP and developer accounts. RuStore and subscriptions remain deferred.

A 1.0 release requires separate approval and successful validation. The 0.9.9 release authorization does not cover 1.0.
