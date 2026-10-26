import chats from "@exp/domain-contract/fixtures/chats.json"
import { PhoneTabBarSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (#EXP-1373) → #EXP-1384 (`chats.json` `shell.tabs` +
// `shell.bar`): THE phone bottom bar, `@exp/ui` `PhoneTabBarSpecimen` (Board
// selected on the board view with the caret, Chats and Team selected, the
// drag lens mid-drag); the app's `?dev=phone-tab-bar` capture renders the same.

const BAR = chats.shell.bar

export const entry: StyleguideEntry = {
  id: `phone-tab-bar`,
  section: `special`,
  owner: `EXP-1384`,
  title: `Phone tab bar`,
  blurb: `THE phone bottom bar ×3: the glass tab capsule (iOS: the app's own glass bar; Android and web-mobile: its look-alike on the FAB chrome recipe) with ${chats.shell.tabs.map((tab) => chats.copy.tabs[tab as keyof typeof chats.copy.tabs]).join(` · `)} in order, each an icon over its label, the selected one on the glass-active pill. A tab is as wide as its label (+ ${BAR.tabPaddingX}px each side, at least ${BAR.tabMinWidth}px), so the board and team names read whole. The Board tab is the ACTIVE board's glyph + name; on the board view it carries the chevrons caret and opens the board menu. The Team tab is the active team's avatar + name (the only way to switch team on a phone); a dot on Chats while a chat needs you (amber), runs (green) or holds an unread reply. DRAG: a press lifts the selection lens (×${BAR.drag.lensScale}) under the finger, it follows the finger along the bar, the tab under it is the candidate (foreground), lift selects it, a lift ${BAR.drag.cancelDistance}px off the bar cancels. Beside it, unchanged, the two-armed new chat / new issue capsule at the same ${BAR.height}px height, ${BAR.gap}px apart. Hidden on detail screens and the pushed Inbox.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PhoneTabBar / PhoneTab`,
      file: `packages/ui/src/phone-tab-bar.tsx`,
      note: `mounted by apps/web components/team/mobile-tab-bar.tsx (md:hidden)`,
    },
    desktop: {
      state: `ok`,
      symbol: `phone_tab_bar`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/phone_tab_bar.rs`,
      note: `the IDE has no phone shell; the styleguide twin paints the same bar`,
    },
    ios: {
      state: `ok`,
      symbol: `MobileTabBar`,
      file: `apps/ios/Exponential/UI/Navigation/MobileTabBar.swift`,
    },
    android: {
      state: `ok`,
      symbol: `BottomNavBar`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/BottomNavBar.kt`,
    },
  },
  island: () => <PhoneTabBarSpecimen />,
}
