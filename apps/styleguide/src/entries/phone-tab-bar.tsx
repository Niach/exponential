import chats from "@exp/domain-contract/fixtures/chats.json"
import { PhoneTabBarSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (#EXP-1373, `chats.json` `shell.tabs`): THE phone bottom
// bar, `@exp/ui` `PhoneTabBarSpecimen` (three states: Chats, Team and Board
// selected); the app's `?dev=phone-tab-bar` capture renders the same.

export const entry: StyleguideEntry = {
  id: `phone-tab-bar`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Phone tab bar`,
  blurb: `THE phone bottom bar ×3: the glass tab capsule (iOS: the system Liquid Glass tab bar; Android and web-mobile: its look-alike on the FAB chrome recipe) with ${chats.shell.tabs.map((tab) => chats.copy.tabs[tab as keyof typeof chats.copy.tabs]).join(` · `)} in order, each an icon over its label, the selected one on the glass-active pill. The Team tab's icon is the ACTIVE team's avatar and its label the team name (the only way to switch team on a phone); a dot on Chats while a chat needs you (amber), runs (green) or holds an unread reply. Beside it, unchanged, the two-armed new chat / new issue capsule at the same 52px height. Hidden on detail screens and the pushed Inbox.`,
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
      symbol: `PhoneTabBar`,
      file: `apps/ios/Exponential/UI/Navigation/MobileTabBar.swift`,
    },
    android: {
      state: `ok`,
      symbol: `PhoneTabBar`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/BottomNavBar.kt`,
    },
  },
  island: () => <PhoneTabBarSpecimen />,
}
