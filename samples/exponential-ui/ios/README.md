# Exponential UI sample: iOS (SwiftUI)

A blank iOS app (`GreenhouseSample.xcodeproj`) hosting the `greenhouse`
surface streamed from the local A2UI JSONL server, with the server's
third-party theme (teal primary, pill buttons) and ONE custom extension
component, `TrendLine`, painted natively (a SwiftUI `Path`). No Exponential
account, no backend of ours: `ExponentialHost` + `JSONLStreamTransport` +
`HostSurface` only, the same shape as `../web/src/main.jsx`.

```bash
bun samples/exponential-ui/server/server.ts      # http://localhost:4190
open samples/exponential-ui/ios/GreenhouseSample.xcodeproj   # run on a simulator
```

- The app fetches `/theme.json` and `/extension.json`, then streams
  `/a2ui.jsonl?once=1` (the surface only; launch argument `-live` keeps the
  stream open: a reading every 3 s, the Refresh button POSTs its action to
  `/action` and the server pushes new readings). `-mode dark`, `-server
  <url>`.
- The project adds ONE dependency, the `ExponentialUI` Swift package, by
  local path (`../../../packages/exponential-ui-swift`): a stand-in for the
  published SwiftPM URL until it exists. The package needs its
  xcframework: `bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh`.
  Once published, replace the local reference with the package URL; nothing
  else changes.
- ATS: `Info.plist` sets `NSAllowsLocalNetworking` for the http server.

Command line:

```bash
xcodebuild -project GreenhouseSample.xcodeproj -scheme GreenhouseSample \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' -derivedDataPath /tmp/greenhouse-dd build
```
