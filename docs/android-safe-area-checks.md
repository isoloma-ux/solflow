# Android safe areas — 0.9.9

Regression reproduced on a clean Android 10/API 29 emulator with the public 0.9.8 APK: `menuDictation` and `modelName` start at x=0. With 0.9.9 they start at x=84 on a 420 dpi screen (32 dp). Android 16/API 36 also receives the system's waterfall inset. This test does not emulate Huawei's actual EMUI firmware or physical glass curvature.

`ScreenInsets` applies baseline padding synchronously, requests insets on attach and handles them at the content root with consistent edge-to-edge window behavior. Insets and column centering are recomputed on size changes; they do not accumulate. Settings offers an optional extra 16 dp margin for vendor firmware without usable waterfall metrics. The map's native container retains its IME handling. Overlay positioning uses the usable parent frame and no longer permits out-of-screen placement.

## Repeatable checks

Use disposable **rootable** Android emulators named `solflow_edges*`, with a standard phone profile, at API 29 and API 36. No physical device is accepted by the script. The script installs the provided APK, replaces its test preferences, changes emulator display settings and captures XML/screenshots. Do not use an emulator containing real user data. Run `adb root` for that emulator first.

```sh
ADB=/path/to/sdk/platform-tools/adb python3 scripts/check-android-screen.py \
  --serial emulator-5582 --apk /path/to/app-release.apk --output /tmp/solflow-screen29
```

It checks actual text/tab margins, drawer navigation, the settings toggle and immediate relayout, waterfall adaptation (API 30+), overlay stability, dragging and rotation. Also inspect the saved screenshots. Separately tested: the recording map with a synthetic recording and a wrapped toolbar within native safe margins. No recognition models, browser sessions or user recordings are used.

Reference: [Android DisplayCutoutCompat waterfall insets](https://developer.android.com/reference/androidx/core/view/DisplayCutoutCompat#getWaterfallInsets()).
