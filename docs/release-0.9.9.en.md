# Sol Flow 0.9.9

[Русский](release-0.9.9.md)

Two usability fixes for Mac and Windows.

- A Rutube connection error or HTTP 403 now shows a hint suggesting that users turn off their VPN or proxy if enabled. Retry submits the same link. The app does not detect VPNs or change network settings. Deleted/private videos, rate limits and cancellations keep their existing messages.
- In the Recording menu, the first Delete click keeps the menu open and changes the item to Really delete?. The second click confirms deletion. Other items still take one click and close the menu. Closing the menu resets confirmation; the existing three-second timeout remains.
- The new interface and version history are available in Russian and English.

Android receives the shared version 0.9.9 (versionCode 45) and change history. The phone already has a separate deletion confirmation dialog and does not import Rutube pages. This update does not change the data format or sync protocol.

MCP is not included in 0.9.9. A separate [project and MCP implementation plan, in Russian](mcp-projects-plan.md) has been prepared.

Validation: local Mac build, 12 downloader regression tests and isolated RU/EN UI checks in dark/light themes at widths 700/920/1440. Deletion tests use synthetic recordings. Public publication, Windows/Android builds and manual runtime tests on those platforms have not been performed for 0.9.9. Mac signing retains the existing Sol Flow identity; Apple notarization is outside this work.
