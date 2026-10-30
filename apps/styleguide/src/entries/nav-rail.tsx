import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-162 — THE navigation rail: Exponential UI's Segmented `bar` with
// `orientation: vertical` and option counts (round-5 contract §3), painted
// by the real React renderer from fixtures/specimens.json
// `exponential-ui-segmented` (the rail case beside the bottom bar it
// stands up from).

export const entry: StyleguideEntry = {
  id: `nav-rail`,
  section: `general`,
  owner: `VAPP-162`,
  title: `Navigation rail`,
  blurb: `The bottom tab bar stood up on a wide screen: Exponential UI's Segmented \`bar\` with \`orientation: vertical\`, no new component. A column of 72 × 56 items (icon over caption, the selected one primary and aria-current=page) on a 72 px rail with a hairline on its trailing edge, content-sized in height (full height is the author's style); ArrowUp/ArrowDown move focus. An option's \`count\` paints a badge centred on its icon's top-end corner (primary fill, 16 px, the number or 99+, none at 0, mirrored under RTL) and reads after the label ("Chats, 3"). Every other variant stacks top to bottom too and ignores counts.`,
  status: {
    web: { state: `ok`, symbol: `SegmentedNative`, file: `packages/exponential-ui-react/src/natives/nav.tsx`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `segmented_bar_item / segmented_badge`, file: `apps/desktop/crates/eui-gpui/src/view/paint.rs` },
    ios: { state: `ok`, symbol: `badge / barItem`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Leaves/ControlLeaves.swift` },
    android: { state: `ok`, symbol: `segmentedBadge`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/Round5.kt` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-segmented" cases={NAV_RAIL_CASES} />,
}

/** The bar with icons, then the rail with counts. */
const NAV_RAIL_CASES = [7, 8] as const
