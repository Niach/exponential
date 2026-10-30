import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-162 — THE pairing code: Exponential UI's `QrCode` native (round-5
// contract §6), painted by the real React renderer from fixtures/
// specimens.json `exponential-ui-qr-code` (the same JSON ui.exponential.at
// and the shots paint). The matrix is the CORE's (`qrMatrix`, a port of
// Project Nayuki's generator), never a platform QR library.

export const entry: StyleguideEntry = {
  id: `qr-code`,
  section: `general`,
  owner: `VAPP-162`,
  title: `QR code`,
  blurb: `A value as a QR code, the pairing code a phone scans off a computer: Exponential UI's QrCode. The matrix comes from the core on every client (qrMatrix: byte mode, the smallest version that fits, the level exact, the lowest-penalty mask; fixtures/qr-code.json locks every module), drawn inside a 4-module quiet zone on a \`size\` px plate (default 160, radius md) with the module edges snapped to device pixels so the outer size stays exact. Dark modules on a light plate ALWAYS: of the plate and module colours the lighter fills the plate, so a dark theme still scans. The label is a muted caption under the plate and the image's accessibility label; without one the code is decorative. An empty value (or one too long for version 40) is the empty plate.`,
  status: {
    web: { state: `ok`, symbol: `QrCodeNative`, file: `packages/exponential-ui-react/src/natives/qr.tsx`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `plate_and_module / dark_runs`, file: `apps/desktop/crates/exponential-ui-gpui/src/paint/qr.rs` },
    ios: { state: `ok`, symbol: `QrCodeLeaf`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Leaves/QrLeaves.swift` },
    android: { state: `ok`, symbol: `QrCodeLeaf`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/Round5.kt` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-qr-code" />,
}
