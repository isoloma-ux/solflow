# Local OpenAI pilot, 2026-09-26

Latest update: installed `mcp-pilot-20260926-access-panel`. The panel separates saved access status, material choices, primary actions, sharing explanation and advanced settings. Connect project explicitly grants permission; selecting checkboxes does not. Enabled projects show Save changes instead. Background updates preserve unsaved choices.

The project context menu offers Connect to AI… (opens choices without granting access), or Manage AI access… and Turn off AI access for enabled projects. The floating menu stays inside the window. Russian and English labels are updated.

Synthetic permission-flow checks and native context-menu/button/dark-theme visual checks passed. Actual user permissions remain off. Original signature verified; app/data backup retained under `.local-tools/mac-mcp-install-20260926-3/`. All 62 source project/recording files are unchanged. No publication or Windows/Android installation.

[Русский](mcp-openai-pilot.md) · [Roadmap](roadmap-1.0.en.md)

Ivan requested OpenAI instead of Claude for the first pilot. “Here” was interpreted as the current Codex task in the ChatGPT desktop app on this Mac; the clarification about ordinary ChatGPT remains unanswered. A chatgpt.com connection is separate and has not been configured.

## Fictional connection

The standard `codex mcp add` command added enabled server `solflow-demo` to `~/.codex/config.toml`. It launches this checkout's `desktop/mcp-server/target/debug/solflow-mcp` over local stdio with `--export-dir` pointing only to `.local-tools/mcp-demo-20260926/export`.

A private backup was saved as `~/.codex/config.toml.before-solflow-20260926-165137.bak`. Existing servers were not removed. Built-in tooling separately refreshed `node_repl.args` during initialization; that change was not reverted. `codex mcp get solflow-demo` confirms the enabled state, stdio transport and fictional-export path.

At this initial fictional stage, no real recordings, Sol Flow settings, audio or credentials were connected. No tunnel, public port, paid API call or Claude connection was used. The client model processes retrieved text; a local MCP process does not mean a local model.

## Verified protocol scenario

The actual server process exposed five tools over JSON-RPC. Search and fetch returned two fictional sources:

| Source | Timestamp | Decision |
| --- | --- | --- |
| План — вымышленная встреча | 00:12.5 | Budget 100,000 rubles, deadline Friday |
| Уточнение — вымышленная встреча | 00:42 | Budget 120,000 rubles, deadline Monday |

The private project's marker produced no search results, and its known document ID was denied. Revoking access without restarting the server removed budget search results and denied a previously retrieved ID. Access to the two fictional recordings was restored for the next step.

Evidence: `.local-tools/mcp-demo-20260926/openai-pilot-preflight.json`. This verifies the MCP protocol; it is **not an in-chat model tool invocation**.

## Direct in-chat verification — passed

On the next turn on 2026-09-26, `mcp__solflow_demo__*` tools appeared in the live tool catalog and were called directly by the model:

- `list_projects` returned one allowed fictional project.
- `search` and `fetch` returned both budget sources with correct text, timestamps and unchanged revisions.
- Private-marker search returned no results; fetching the private ID was denied.
- After temporary revocation through the demo utility, direct in-chat search returned nothing and the previously accessible ID was denied.
- After restoring access, `list_recordings` returned two records. Fictional access remains enabled.

Reading, search, isolation and live revocation are now verified through this task's connected tools. The earlier `openai-pilot-preflight.json` remains protocol-preflight evidence; direct tool calls are recorded in the task history.

## Installed real-project pilot — 2026-09-26

Local build `mcp-pilot-20260926` is installed at `/Applications/Sol Flow.app` on Ivan's Mac, retaining version 0.9.9 and the original Sol Flow Signing certificate. The app, MCP binary and library passed `codesign --verify --deep --strict`. Nothing was published.

The previous app and a verified full data backup are retained under `.local-tools/mac-mcp-install-20260926-1/`. Last-minute dictation changes were also saved in `data-latest-before/`. All 59 project/recording files retained their SHA-256 hashes after installation.

The native “Тест MCP” project permission panel was opened and only transcripts enabled with Ivan's authorization. The managed index contains one project, one recording and 263 transcript fragments. Summaries, maps, analyses and other projects remain closed; audio is excluded.

The installed MCP executable passed actual stdio/JSON-RPC checks: five tools, one allowed recording, matching search/fetch text and revision, and source timestamps. Clicking Revoke in the native app immediately returned empty project/search lists and denied fetching a previously accessible ID in the same server process. Enabling transcripts again restored access without restarting that process. Transcript access remains enabled.

Evidence is retained in `installed-mcp-check.json` and `data-preservation-check.json` alongside the backup. The standard CLI registered `solflow`, using the installed binary and `~/Library/Application Support/ru.ivansolomin.solflow/mcp-export`. Configuration was backed up to `~/.codex/config.toml.before-solflow-real-20260926-173255.bak`; existing servers were preserved.

**Verification boundary:** the installed app, native permission controls and real-recording protocol passed. This turn's model tool catalog still exposes only `solflow-demo`; direct calls to the new `solflow` must be verified after client tool refresh. Ordinary chatgpt.com is not configured. Cold/warm native `solflow://` link handling remains untested.

Add or move new recordings into the allowed project and wait for transcription. This is a logical Sol Flow project, not a watched Finder folder; arbitrary TXT/DOCX/PDF Inbox import remains unimplemented.


Final native UI check: dictation status reached Ready; the diagnostic console displayed no errors. No microphone recording was started during this check.

## Official connection paths

[Local MCP in Codex/the ChatGPT desktop app](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) supports stdio through shared configuration. [Ordinary ChatGPT via Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels) requires a workspace-associated tunnel, Platform tunnel permissions and developer-mode access. The latter was not configured.

To remove only this demo connection: `codex mcp remove solflow-demo`. This preserves fictional files and Sol Flow recordings. Do not overwrite subsequent configuration changes by restoring the entire old backup.

## Second real recording and project markers — 2026-09-26

A webinar added by the user appeared automatically as the second recording in “Тест MCP”. All 634 fragments were fetched through the installed MCP protocol; their text matched the source transcript. A separate Russian summary was saved under `11_Sol_Flow/MCP_пилот/Саммари_вебинара_2026-09-26.md` without changing the recording.

Local build `mcp-pilot-20260926-sidebar` is installed. Projects granting any material type now have a purple highlight and an accessible AI badge. Native dark-theme checks passed for startup, switching projects, immediate revocation/restoration and matching MCP responses. Light-theme colors and English labels are included. The badge denotes saved permission, not an active client connection.

Original signing identity verified; previous app and current data retained in `.local-tools/mac-mcp-install-20260926-2/`. All 59 source project/recording files remained unchanged. The real server still has not appeared in this chat's tool catalog; client connection refresh remains pending. Windows/Android were not installed during this local Mac iteration; MCP serving and permission controls remain desktop-pilot features.


## Generic client setup and interview — 2026-09-26

Installed local build `mcp-pilot-20260926-clients`. The AI connection setup button now offers instructions for ChatGPT / Codex (default), Claude Desktop, or another MCP client. Selection changes instructions and parameter format only (OpenAI TOML, Claude JSON, generic STDIO parameters); it does not change project grants or detect the active client. Existing access panel layout and AI markers are preserved, with Russian and English text.

OpenAI instructions were checked against https://learn.chatgpt.com/docs/extend/mcp. Desktop and web ChatGPT are explicitly distinguished. This conversation still exposes only `solflow-demo`; direct use of the registered real `solflow` connector needs a client tool refresh.

The installed MCP server returned all 607 fragments of Ivan Solomin's Atom interview; every text and timestamp matched the source transcript. A summary and standalone interactive HTML mind map were saved in `11_Sol_Flow/MCP_пилот/`. No recording or generated result in Sol Flow was overwritten. Browser visual testing of the map was blocked by the browser file-protocol policy; no workaround was attempted. JavaScript syntax and DOM-model checks of eight branches, keyboard, text mode, and print action passed; these do not establish browser rendering.

Build, original signing identity, native setup button, and OpenAI configuration verified. Controller checks cover client switching, TOML/JSON formats, unchanged grants, and existing permission flows. All 62 project/recording files and every permission row were preserved. Backup and evidence: `.local-tools/mac-mcp-install-20260926-4/`. This is a local Mac update, not a published release.


## Setup request — 2026-09-26

Ivan reports the Windows app and MCP work after correcting a full TOML block pasted into the command field. This is user-reported Windows testing.

The shared Mac/Windows UI now leads with Copy setup request. It includes exact local paths, client-specific configuration instructions, backup and preservation of other settings, and MCP initialize/tools/list/list_projects verification without reading recordings. Automatic execution requires an assistant with access to files and processes on that computer. Copying alone changes neither client configuration nor project permissions. Manual setup is collapsed, with separate raw command/argument copy buttons; full configuration is explicitly for a settings file. UI, prompts and shared guide are bilingual. Android retains desktop setup guidance rather than running a local MCP server.

Validated: 18 Mac/Windows/special-path prompt variants, RU/EN coverage, clipboard separation and unchanged permissions, real browser preview, Mac compilation and original signing identity. Mac was updated locally with a backup (mcp-setup-prompt-20260926). The previously delivered Windows EXE does not contain this button; no new Windows installer or Android APK was built for this change. End-to-end execution of the request by an independent assistant from a clean configuration remains untested.
