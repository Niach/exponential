import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-162 — the pairing scanner: Exponential UI's `QrScanner` native
// (round-5 contract §7), the viewfinder CHROME around a HOST-owned camera,
// painted by the real React renderer from fixtures/specimens.json
// `exponential-ui-qr-scanner`. The styleguide has no camera, so it shows
// the placeholder every client paints without one.

export const entry: StyleguideEntry = {
  id: `qr-scanner`,
  section: `general`,
  owner: `VAPP-162`,
  title: `QR scanner`,
  blurb: `The other half of pairing: Exponential UI's QrScanner, a square viewfinder as wide as its column up to 280 px (radius xl, clipping) with four primary corner brackets (24 px strokes, 3 px wide, rounded caps) and a 2 px scan line sweeping top to bottom and back every 2 s while a feed shows (still at the centre under reduced motion or disabled). The CAMERA IS THE HOST'S: each renderer asks its host through its own hook (web: \`host.qrScanner.start(feed, formats, onValue) → stop\`; SwiftUI an environment value; Compose LocalQrScanner; gpui a host trait), the host draws its live video into the viewfinder and reports decoded strings, and the scanner fires \`scanned {value}\` once per distinct value. No camera = no feed and no line: the scan icon and "Camera unavailable" in the middle. The hint is a muted line under it; disabled stops the camera and dims the chrome.`,
  status: {
    web: { state: `ok`, symbol: `QrScannerNative`, file: `packages/exponential-ui-react/src/natives/qr.tsx`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `QrScannerSpec`, file: `apps/desktop/crates/eui-gpui/src/paint/qr.rs` },
    ios: { state: `ok`, symbol: `ExponentialQrScanner / QrScannerFeed`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Leaves/QrLeaves.swift` },
    android: { state: `ok`, symbol: `QrScannerLeaf`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/Round5.kt` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-qr-scanner" />,
}
