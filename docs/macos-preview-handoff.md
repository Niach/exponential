# macOS Device-Preview Handoff

This is the to-do for finishing + testing the **device preview / annotate / report**
feature on the macOS app (`apps/ios/ExponentialMac`). The shared contract, web UI,
and the **Linux** app are done, committed (`a8826d2` on `master`), and deployed to
staging (`next.exponential.at`). The macOS Swift was written and **statically
reviewed but never compiled** (it was authored on a Linux box with no Xcode), so
the work here is: compile it, fix what the compiler finds, finish two first-cut
TODOs, and test the loop against staging.

## 0. Prereqs on the Mac

- Xcode + command-line tools (`xcode-select -p` must succeed).
- `tuist` (the project is generated, not committed): `tuist generate` in `apps/ios`.
- Node LTS (for `serve-sim`, used by the iOS backend) — `node -v`.
- `serve-sim` reachable via `npx serve-sim` (Apple-Silicon only).
- Android SDK + `emulator`/`adb` on PATH (or `ANDROID_HOME` set) **if** you test the
  Android target on the Mac; otherwise skip Android (it's the Linux app's strong suit).

## 1. Generate + build

```bash
cd apps/ios
tuist generate
# Build the macOS app
xcodebuild -scheme Exponential-macOS -configuration Debug build
# Run the geometry parity tests (the cross-language contract)
tuist test ExpCore          # or: xcodebuild test -scheme ExpCore
```

`tuist test ExpCore` runs `AnnotationGeometryTests.swift`, a 1:1 port of
`packages/widget/src/annotate/shapes.test.ts`. **It must pass** — it's the parity
gate that keeps Swift/Zig/TS annotation math identical. If it diverges, fix
`apps/ios/ExpCore/Sources/Annotate/AnnotationGeometry.swift` to match `shapes.ts`
exactly (`strokeWidthFor = max(3, round(maxEdge*0.0035))`, `arrowSpread = π/7`,
`arrowHeadLength = max(10, sw*4)`, `isDegenerate` minDrag 4).

### New / changed Swift files to expect compile errors in
All under `apps/ios/`:
- `ExpCore/Sources/Annotate/AnnotationGeometry.swift` (+ `ExpCore/Tests/AnnotationGeometryTests.swift`)
- `ExpCore/Sources/API/HTTPClient.swift` (added `postMultipart`), `ExpCore/Sources/API/ProjectsApi.swift` (added `updatePreviewConfig` + input types)
- `ExpCore/Sources/DB/{DatabaseManager,Entities}.swift` (new `preview_config` column + `ProjectEntity.previewConfig`)
- `ExponentialMac/MacAnnotationRenderer.swift`, `MacFeedbackReporter.swift`, `MacPreviewConfig.swift`,
  `MacPreviewController.swift`, `MacPreviewBackends.swift`, `MacPreviewHost.swift`,
  `MacPreviewAnnotateView.swift`, `SendFeedbackSheet.swift`, `MacPreviewPane.swift`,
  `MacPreviewDoctor.swift`, `MacProjectPreviewSettingsView.swift`
- `ExponentialMac/{MacShell,MacAppDependencies,ExponentialMacApp,MacWorkspaceSettingsView}.swift` (pane wiring + teardown)
- `apps/ios/Project.swift` (new `ExpCoreTests` target wired into the `ExpCore` scheme)

Likely review-flagged spots to check during the first build (from the static review):
- `WKWebView.setValue(false, forKey: "drawsBackground")` — KVC for a transparent
  background; confirm it's accepted on the deployment SDK.
- `Data.firstRange(of:)` (MJPEG SOI/EOI scan in the iOS backend) — needs the
  stdlib collection-search API (macOS 14+); fine for the target, just unexercised.
- `PreviewShell.augmentedEnvironment` SDK/Homebrew path discovery — verify the
  Android SDK + `node`/`npx` paths resolve on your Mac.

## 2. Finish the two first-cut TODOs

1. **Android embed → ScreenCaptureKit.** `MacPreviewBackends.swift`
   `AndroidPreviewBackend` currently boots + installs + launches the emulator and
   runs it **alongside** (its own window); annotation frames are already
   pixel-exact via `adb exec-out screencap -p`. Promote the embed to an `SCStream`
   filtered to the emulator window → `AVSampleBufferDisplayLayer` mounted in the
   pane (there's a marked `TODO`). Triggers the screen-recording TCC prompt once.
2. **iOS embed → AVSampleBufferDisplayLayer.** `IOSSimPreviewBackend` renders the
   `serve-sim` MJPEG stream as an `<img>` in a `WKWebView` (first cut). The cleaner
   path is decoding the MJPEG into an `AVSampleBufferDisplayLayer` for a crisp
   frame-grab (marked `TODO`). **Verify the real `serve-sim` contract** while you're
   here: the code assumes flags `--scheme/--workspace/--simulator/--bundle-id/--detach/--kill`
   and JSON `{udid, mjpeg, ws}` from the plan — confirm against the installed tool
   and adjust `IOSSimPreviewBackend` if they differ.

## 3. Two low-severity teardown fixes (from the review)

In `MacPreviewController.swift` / `MacPreviewBackends.swift` `stop()`:
- Android `stop()` fires `adb emu kill` on a detached queue and returns without
  awaiting it → a rapid project/target switch can race the next boot's
  `freePort(8554)`. Await the kill (or gate the next start on the prior teardown).
- Web `stop()` only frees `url.port`; a `url` without an explicit port (e.g.
  `http://localhost`) frees nothing. Force-free the resolved port (the Linux side
  always frees 5173/8554/3100). Mirror that.
- Cosmetic: macOS folds `bundleId` into the iOS trust-hash command set while Linux
  hashes `scheme`/`workspace` — harmonize so the trust prompt re-fires on the same
  inputs cross-platform (both already re-prompt on the security-relevant fields).

## 4. Test the loop against staging

The build/run config + dogfood project are already live on staging.

1. Point the macOS app at **`next.exponential.at`** (the instance-URL prompt) and
   sign in. The public **Feedback** workspace now contains an **Exponential (EXP)**
   project linked to `niach/exponential` with four run targets (web / android /
   ios-staging / ios-prod) defined in `.exponential/config.json`.
2. The desktop must have the repo cloned for the preview to read
   `.exponential/config.json` from the working tree (`~/Library/Application
   Support/Exponential/repos/<slug>`). If the agent hasn't cloned it yet, that's the
   one ordering gap to watch — trigger a clone (assign the device an EXP issue) or
   confirm the preview falls back gracefully when the working tree is absent.
3. Open the EXP project → **Preview pane** → pick a target:
   - **web** → builds `apps/web` (`bun run dev`, :5173), embeds it in `WKWebView`;
     the in-page feedback widget works there.
   - **ios-prod / ios-staging** → `serve-sim` boots the Simulator, streams MJPEG;
     confirm `ios-staging` builds the `Exponential-Staging` scheme / `at.exponential.staging`.
   - **android** (if SDK present) → emulator boots, installs `at.exponential`,
     launches `com.exponential.app.MainActivity`.
4. Toggle **Annotate**, draw rect/pen/arrow, **Send feedback** → confirm a new issue
   lands in the **EXP** project on staging with the flattened screenshot embedded,
   attributed to **you** (the logged-in dev), not a widget bot. It should sync back
   into the macOS issue list via Electric.
5. The trust gate: first run of a target prompts "Trust preview commands for
   `niach/exponential`?"; editing a command in the repo file re-prompts, a display
   rename does not.

## 5. Release notes

macOS distribution still needs the Developer ID cert + `notarytool` (the dylib
bundling/ad-hoc-signing is already in place). The preview feature itself is
store-policy neutral (no billing UI; local-only emulation). Nothing here changes
the cloud env posture.
