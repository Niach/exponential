import fixture from "../../../../packages/domain-contract/fixtures/computer-use-banner.json"

import { svgComputer } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1273 — THE computer-use banner: the bulk bar's capsule, shown by the
// desktop app at the bottom of the primary display while a session drives
// the screen and the person's keyboard and pointer are held. Copy and rules
// are the contract fixture's; the IDE draws the same bar
// (`computer_indicator::banner_bar`). Hand-drawn with the bulk bar's own
// classes, so the two capsules can never drift apart.

function banner(label?: string): string {
  const value = label === undefined ? `` : `<span class="value">${label}</span>`
  return [
    `<div class="cmp-bulk-bar">`,
    `<span class="item">${svgComputer}${fixture.copy.title}</span>`,
    value,
    `<span class="item"><kbd class="cmp-kbd">${fixture.copy.key}</kbd>${fixture.copy.abort}</span>`,
    `</div>`,
  ].join(``)
}

export const entry: StyleguideEntry = {
  id: `computer-use-banner`,
  section: `special`,
  owner: `EXP-1273`,
  title: `Computer use banner`,
  blurb: `The one signal that an agent has the screen: while a coding session drives this machine (foreground only, since EXP-1273) the person's keyboard and pointer are held, and the bulk bar's capsule floats in a top-most, unfocused, click-through window 16 px above the bottom of the primary display for exactly that long. The nav-computer glyph with "${fixture.copy.title}" as one item, the session's label as the bold value, then "${fixture.copy.key}" as a kbd chip and "${fixture.copy.abort}": Esc is the way out, the hold ends on the key itself and the session's turn stops. One banner per machine; it goes with the release (Esc, the session's end, the switch off, 30 s idle).`,
  status: {
    web: { state: `n/a`, note: `The screen is the device's: web shows nothing while a device is driven.` },
    desktop: {
      state: `ok`,
      symbol: `computer_indicator::banner_bar`,
      file: `apps/desktop/crates/ui/src/computer_indicator.rs`,
    },
    ios: { state: `n/a`, note: `Phones do not drive a screen.` },
    android: { state: `n/a`, note: `Phones do not drive a screen.` },
  },
  render: () => `<div class="cmp-stack">${banner(`EXP-1273`)}${banner()}</div>`,
}
