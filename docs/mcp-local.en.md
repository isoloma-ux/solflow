# Local Sol Flow MCP

> Update 2026-09-26: Android project permission controls now sync with updated desktop apps. Open a project or long-press it in the drawer. Purple and AI mark enabled projects. Changes apply on other devices after sync; an offline computer retains its previous access. MCP serving remains on the computer. [Current implementation and checks](mcp-android-access.md). The following sections retain earlier pilot history.


Update, 2026-09-26: the first pilot now targets the current ChatGPT/Codex app. The fictional server is registered; direct in-chat calls, search, private-project isolation and live revocation passed. Real projects are not connected yet. [Pilot status](mcp-openai-pilot.en.md).

[Русский](mcp-local.md) · [Roadmap and outstanding work](roadmap-1.0.en.md)

Status on 2026-09-26: implemented locally on `work/1.0-mcp`; the app version remains 0.9.9. The server and desktop app build locally in debug, and automated checks pass. The installed app has not been replaced, Claude Desktop is not connected and nothing has been published.

## Access model

Each Sol Flow project has an MCP panel with separate permissions for transcripts, summaries, mind maps and analyses. All are off by default. Saving access creates a managed `mcp-export/export.sqlite3` index in the application data directory. The existing `projects.json`, `meta.project` and `meetings/<id>/` formats are preserved.

Only Sol Flow reads original records and updates the index. The separate `solflow-mcp` process receives the export directory and opens its database read-only. It uses the official Rust SDK RMCP 3.4.1 over stdio, without a network listener. Every tool call opens a fresh snapshot and checks permissions again. This is an API boundary, not an OS sandbox for a process running as the user.

| Tool | Result |
| --- | --- |
| `list_projects` | Allowed projects and index status |
| `list_recordings` | Finished recordings, pages of up to 100 records |
| `search` | Up to 50 text fragments with source URL, date, speaker, timestamps and revision |
| `fetch` | Fragment by ID, with another permission check and revision-change detection |
| `get_project_overview` | Project name, first recording page and index status |

Search supports Cyrillic, word prefixes and phrases enclosed in double quotes. It does not implement full Russian morphology. Fragments contain up to 2400 characters; timestamps refer to the original segment, not the exact word within a long segment. Summaries, maps and analyses carry `generated: true` and are not verbatim speech.

Audio, settings/keys, question history, translations and unfinished transcripts are excluded. Tools do not write results back, activate the microphone, run models or download URLs. `stderr` receives tool audit events without queries or meeting content; there is no persistent in-app audit log yet.

## Refresh and revocation

Saving metadata, transcripts or analyses, moving/deleting recordings, editing projects and incoming synchronization invalidate the index. A background queue rebuilds it. Refresh removes the previous searchable text first; corrupt or changing sources leave the export stale and show an error. A refresh button retries. Sol Flow also rebuilds the index on startup.

Revocation transactionally removes that project's accessible fragments. Subsequent tool calls observe it without restarting the MCP server; an in-flight call may finish. Revocation cannot recall text already received by a client, conversation history or manually created copies.

The explicit text-copy button exports **all currently allowed projects** to `mcp-export/copies/<hash>/`, with Markdown fragments and a `manifest.json` mapping IDs to filenames. Copies are separate from the live MCP source and remain after revocation. Unrelated user files next to the index are preserved.

The local server makes no external requests, but Claude may send retrieved text to its cloud service. The permission panel explains this. Use fictional recordings for the initial pilot.

## Build and connect

Development requires Rust, Python 3 and the normal Sol Flow build dependencies. From the repository root:

```sh
python3 scripts/check-mcp.py
python3 scripts/prepare-mcp.py
cargo check --locked --manifest-path desktop/src-tauri/Cargo.toml
```

`prepare-mcp.py` copies the platform-suffixed executable to `desktop/src-tauri/binaries/`. Before a normal Tauri installer build, run `python3 scripts/prepare-mcp.py --release`. `bundle.externalBin` includes the component, and Windows/macOS workflows prepare it. These scripts do not install, sign or publish anything. If full Xcode prompts for license acceptance, existing Command Line Tools can be selected per command with `DEVELOPER_DIR=/Library/Developer/CommandLineTools`; this does not accept an Xcode license.

Manual pilot after preparing a separate test build:

1. Create one project with two fictional finished recordings and a second private project with a distinctive secret marker.
2. Enable only transcripts for the first project and wait for refresh. Keep the second project private.
3. Use the Claude configuration button. It displays JSON containing this installation's absolute executable path and `--export-dir` argument; it does not change existing client settings.
4. Follow [Claude's official local-server instructions](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop). Back up the configuration and merge `solflow` into the existing `mcpServers` object, preserving other servers; restart Claude.
5. Ask: “Compare budget decisions across the two meetings. For each change, cite the recording title, date, speaker, exact quote and timestamp.” Check the response against source fragments.
6. Verify the private marker cannot be found. Revoke the first project's access and make another tool call: its text must no longer be returned. Earlier conversation messages will remain.
7. Test `solflow://recording/<id>?t=<seconds>` with the app open and closed. The handler opens the recording and seeks without autoplay. OS scheme registration needs an installed-package test; handler code alone does not prove links work in every client.

Keep the executable at a stable path; moving the app requires updating configuration. After initial permission setup, MCP can read the last prepared index while Sol Flow is closed, but synchronization updates and revocation require Sol Flow. Do not pass the original recording directory as the export directory.

### Standalone fictional export

You can pilot the server before installing a new Sol Flow build. `desktop/mcp-core/examples/demo_export.rs` creates fictional data in a **new** absolute directory, rejecting an existing directory at creation:

```sh
cargo run --locked --manifest-path desktop/mcp-core/Cargo.toml --example demo_export -- create /absolute/new-demo-directory
```

Configure a separate `solflow-demo` server with the absolute path to `desktop/mcp-server/target/debug/solflow-mcp` and arguments `--export-dir /absolute/new-demo-directory/export`. Replace `create` with `revoke` to test revocation or `restore` to restore access, keeping the same directory. Both require the demo marker. The private project is never enabled.

The current checkout already has `.local-tools/mcp-demo-20260926/` with a ready `claude-config.json`. Creation, revocation and restoration were checked: two accessible fragments, then zero, then two again. Expected answer: fictional launch Alpha's budget changed from 100,000 to 120,000 rubles and its deadline from Friday to Monday, with sources at 12.5 and 42 seconds. The private project's marker “ФИОЛЕТОВЫЙ БАРСУК” must not be returned. These demo URLs do not refer to actual recordings in the installed app; OS links require a separate test.

## Verification

On Mac, 2026-09-26:

- Standalone server build and 7 tests, including an actual SDK process/protocol exchange, private-project isolation, live revocation, original-byte preservation, corrupt sources and rejected symlinks. Search covers word prefixes and exact phrases.
- Desktop `cargo check` with and without default features, followed by a full `cargo build --offline --locked` with default features and successful linking. This is a local debug build, not a signed installer or a running-app test.
- Android `:app:compileDebugKotlin`; `scripts/check-safeguards.py`: 14 Rust and 42 Kotlin checks.
- Project panel in a real browser with a fictional backend: grant/revoke, project switching, configuration, RU/EN and no JavaScript errors. This does not test native Tauri IPC.

Still pending: full native flow, real Claude, OS links on Mac/Windows, Windows file locking, nested-component signing, clean installation and updates. Real client settings and user recordings were not changed during these checks. ChatGPT needs separate connection-method and account-permission verification; compatibility is not claimed.

Run protocol tests through `scripts/check-mcp.py`: plain `cargo test` skips the ignored protocol test that requires a built executable. Do not treat that shorter run as full MCP verification.

Update: real Claude and Android/Mac sync were subsequently verified. See the [current pilot report](pilot-cross-platform-20260926.en.md); earlier unchecked items above describe the initial development pass.
