import chats from "@exp/domain-contract/fixtures/chats.json"
import { PhoneBoardMenuSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1384 (`chats.json` `shell.boardMenu`): THE phone board menu,
// `@exp/ui` `PhoneBoardMenuSpecimen` (the menu OPEN over the strip it grew
// from); the app's `?dev=phone-board-menu` capture renders the same.

const MENU = chats.shell.boardMenu

export const entry: StyleguideEntry = {
  id: `phone-board-menu`,
  section: `special`,
  owner: `EXP-1384`,
  title: `Phone board menu`,
  blurb: `THE phone's only board switch ×3: on the board view the Board tab MORPHS the whole bottom strip (the tab capsule and the new chat / new issue capsule) into one glass context menu panel, growing from the strip's frame (same left, right and bottom edges, the corner radius to ${MENU.radius}px) at ${MENU.motion.open} / ${MENU.motion.openEase} while the rows fade in; it collapses back at ${MENU.motion.close} / ${MENU.motion.closeEase} (reduce motion = a cross-fade). Rows: the active team's boards in sidebar order (no team name, no header), ${MENU.rowHeight}px each, the board glyph + name, the active board on the glass-active pill with the check; then the muted "${chats.copy.boardMenu.newBoard}" row for a viewer who may create boards. Past ${MENU.maxVisibleRows} rows they scroll inside the panel. A tap outside (a transparent scrim) or back collapses it.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PhoneBoardMenuStrip`,
      file: `packages/ui/src/phone-board-menu.tsx`,
      note: `mounted by apps/web components/team/mobile-tab-bar.tsx around the strip`,
    },
    desktop: {
      state: `ok`,
      symbol: `render`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/phone_board_menu.rs`,
      note: `the IDE has no phone shell; the styleguide twin paints the open menu`,
    },
    ios: {
      state: `ok`,
      symbol: `MobileTabBar`,
      file: `apps/ios/Exponential/UI/Navigation/MobileTabBar.swift`,
    },
    android: {
      state: `ok`,
      symbol: `PhoneBoardMenu`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/BottomNavBar.kt`,
    },
  },
  island: () => <PhoneBoardMenuSpecimen />,
}
