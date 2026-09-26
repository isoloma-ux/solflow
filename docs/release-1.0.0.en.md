# Sol Flow 1.0.0

[Русский](release-1.0.0.md)

Connect Sol Flow projects to AI through local MCP. This is a shared Windows, Mac and Android update: the server runs on the computer, while the phone manages permissions and syncs materials.

## Changes

- Per-project access to transcripts, summaries, maps and analyses. Access is off by default.
- Purple highlighting and an AI badge for shared projects. Use the project panel or right-click menu on desktop; use the panel or long-press a project on Android.
- Project permissions sync across devices. Changes take effect after both devices sync. Update all devices: older versions cannot manage the new permissions.
- A bundled local MCP component on Mac and Windows searches and reads finished materials with source names and timestamps. Every new request checks whether access is still allowed.
- Copy setup request generates instructions with this computer's exact paths. An assistant with local file access can configure the chosen client. Manual setup separates the command and arguments; full JSON/TOML belongs only in a settings file.
- Updated Russian and English guide with dedicated Android instructions and real screenshots. Reopen it through About → Sol Flow guide.
- Fixed Windows path-prefix handling in MCP while retaining protection against filesystem link escapes.

## Connect

1. On your computer open a project → Project access for AI. Choose materials and select Connect project.
2. Open Client setup and more options → AI connection setup. Select ChatGPT / Codex, Claude Desktop or another MCP client.
3. Copy the setup request and send it to an assistant on the same computer. Automatic setup requires local file and process access; an ordinary chat without it can only explain manual steps. Alternatively expand Set up manually and copy each field separately.
4. After setup and any required client restart, ask for the project list, read a test recording and request a fact with a source and timestamp.
5. Revoke access and test a fresh read. Previously retrieved answers remain in chat history.

The purple badge indicates project permission, not a successful connection to a particular client. Local MCP configuration does not automatically move from Mac to Windows. Android does not run the server. Browser ChatGPT/Claude and mobile chats need a separate connection method; Sol Flow does not install a cloud server or tunnel.

## Data and limitations

MCP is not hosted on ivansolomin.ru. The component runs locally without opening a public port. A connected cloud AI processes requested permitted texts through its provider. Audio and credentials are excluded from the MCP export. Search and read operations do not change originals; AI answers are not automatically written back to Sol Flow. An explicitly saved text copy remains on disk after access is revoked.

## Download and update

Release files: [Sol Flow 1.0.0](https://github.com/isoloma-ux/solflow/releases/tag/v1.0.0).

- Windows 10/11 x64: `SolFlow_1.0.0_x64-setup.exe`.
- Mac Apple silicon, macOS 14+: `SolFlow_1.0.0_macOS.zip`.
- Android 8+, arm64: `SolFlow_1.0.0.apk` (versionCode 46).

Install over the existing version without deleting data. Mac and Windows receive the built-in update offer; Android downloads an APK and requests system installation confirmation. Recordings, projects and models remain in place. Recheck microphone and accessibility permissions if the operating system requests them again.

Mac retains the existing private signing certificate; it is not notarized by Apple. Windows has no Authenticode signature. Updater package signatures and APK signing are checked separately and do not imply store approval. Distribution is through GitHub and the built-in updater, not App Store or Google Play.

## Validation boundaries

The pilot covered local MCP, project isolation, revocation, Android ↔ Mac sync and Claude Desktop with fictional materials. The user confirmed Windows installation and MCP operation. The latest setup-request button was tested in the installed Mac build and RU/EN browser checks, including Windows paths; successful independent execution by a third-party assistant from a clean configuration is not guaranteed.

Final packages are published after CI and checks of signatures, versions, archive contents and the updater manifest. A physical Huawei device, every sync-conflict scenario, the second cloud provider and clean installation on every Windows configuration were not separately tested.
