import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1629 — Exponential UI's `Avatar` native, painted by the real React
// renderer from fixtures/specimens.json `exponential-ui-avatar`. The rule is
// fixtures/avatar.json: the app's avatar palette and initials ×5 renderers.

export const entry: StyleguideEntry = {
  id: `avatar`,
  section: `general`,
  owner: `EXP-1629`,
  title: `Avatar`,
  blurb: `A person as a round picture or their initials: Exponential UI's Avatar, the app's own palette (fixtures/avatar.json). Sizes sm, default, lg, xl (48 px); the letters at half the diameter, regular weight. A seed (a user id) picks one of 8 hues: the hue at 20% behind initials in it. No seed: muted. A picture wears a hairline border ring.`,
  status: {
    web: { state: `ok`, symbol: `AvatarNative`, file: `packages/exponential-ui-react/src/natives/media.tsx`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `avatar`, file: `apps/desktop/crates/eui-gpui/src/paint/natives.rs` },
    ios: { state: `ok`, symbol: `AvatarLeaf`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Leaves/MediaLeaves.swift` },
    android: { state: `ok`, symbol: `AvatarLeaf`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/MediaLeaves.kt` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-avatar" />,
}
