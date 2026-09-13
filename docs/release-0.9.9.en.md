# Sol Flow 0.9.9

[Русский](release-0.9.9.md)

Android display spacing fixes and usability improvements for Mac and Windows.

- A Rutube connection error or HTTP 403 now shows a hint suggesting that users turn off their VPN or proxy if enabled. Retry submits the same link. The app does not detect VPNs or change network settings. Deleted/private videos, rate limits and cancellations keep their existing messages.
- In the Recording menu, the first Delete click keeps the menu open and changes the item to Really delete?. The second click confirms deletion. Other items still take one click and close the menu. Closing the menu resets confirmation; the existing three-second timeout remains.
- The new interface and version history are available in Russian and English.

On Android, fixed missing margins on older firmware: text, menus and bottom tabs no longer start at the very edge of the screen. Baseline padding is applied before the system reports its safe area. Main screens and the recording map account for system bars, cutouts, waterfall edges and the keyboard. The floating button stays within reach when dragged or after rotation.

Settings → Appearance now includes “Extra screen edge spacing”: an additional 16 dp on each side for firmware that does not report the size of its curved edges. Automatic spacing works without enabling this option. Both Russian and English interfaces are included.

Android versionCode is 45. The phone already has a separate deletion confirmation dialog and does not import Rutube pages. This update does not change the data format or sync protocol.

MCP is not included in 0.9.9. A separate [project and MCP implementation plan, in Russian](mcp-projects-plan.md) has been prepared.

Validation: the user tested Mac 0.9.9; the local Android build uses the existing signing key. On a clean Android 10 emulator, missing margins were reproduced in 0.9.8 and verified as fixed in 0.9.9. Android 16 was also tested with waterfall emulation, extra spacing, the floating button and screen rotation. The physical Huawei ELS-N39 is not connected to the test machine, so validation of that particular firmware remains with its owner. Desktop checks: 12 downloader regression tests and isolated RU/EN UI checks in dark/light themes at widths 700/920/1440. Deletion tests use synthetic recordings.

Release preparation status: publication is authorized after validation; Windows/Mac/Android build results and verification of all seven release files are recorded separately before publication. Windows has not been tested manually. Mac signing retains the existing Sol Flow identity; Apple notarization and Windows Authenticode are outside this update.
