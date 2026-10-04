import { JumpToBottomButton } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1191: the arrow-only "back to the newest row" circle of the run
// transcript, the real `@exp/ui` button.

export const entry: StyleguideEntry = {
  id: `jump-to-bottom`,
  section: `general`,
  owner: `EXP-1191`,
  title: `Jump to bottom`,
  blurb: `The one control that returns a bottom-anchored feed (the run transcript) to its newest row, identical on all four clients: a 32 px circle in the floating glass (the popover fill, the card hairline, no shadow) holding only the down arrow (ui-arrow-down, 16 px, the secondary foreground). It sits centred over the feed, 12 px above the composer, fades and scales in while the reader is scrolled up and out once they are back at the tail. Click scrolls to the end and re-arms follow; the label "Jump to bottom" is its tooltip and accessible name.`,
  status: {
    web: {
      state: `ok`,
      symbol: `JumpToBottomButton`,
      file: `packages/ui/src/jump-to-bottom-button.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `controls::jump_to_bottom_button`,
      file: `apps/desktop/crates/ui/src/controls.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `JumpToBottomButton`,
      file: `apps/ios/ExpUI/Sources/JumpToBottomButton.swift`,
    },
    android: {
      state: `ok`,
      symbol: `JumpToBottomButton`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/JumpToBottomButton.kt`,
    },
  },
  island: () => (
    <div className="flex h-24 items-end justify-center rounded-lg border border-glass-stroke-card bg-card/40 pb-3">
      <JumpToBottomButton visible onClick={() => {}} />
    </div>
  ),
}
