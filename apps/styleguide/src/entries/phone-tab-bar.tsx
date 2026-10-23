import chats from "@exp/domain-contract/fixtures/chats.json"
import { PhoneTab, PhoneTabBar, TeamAvatar, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (#EXP-1373, `chats.json` `shell.tabs`): THE phone bottom
// bar's tab capsule, the real `@exp/ui` `PhoneTabBar` the web-mobile shell
// mounts (components/team/mobile-tab-bar.tsx): Board · Devices · Chats ·
// Team, each an icon over its label, the selected one on the glass-active
// pill; the Team tab = the active team's avatar + name; the Chats dot =
// unread / live. Two states: Chats selected with its amber dot, Team
// selected.

const BoardIcon = conceptIcon(`nav-issues`)
const DevicesIcon = conceptIcon(`nav-devices`)
const ChatsIcon = conceptIcon(`nav-threads`)

function Bar({ selected }: { selected: `chats` | `team` }) {
  return (
    <PhoneTabBar>
      <PhoneTab label={chats.copy.tabs.board} icon={<BoardIcon className="size-5" />} />
      <PhoneTab label={chats.copy.tabs.devices} icon={<DevicesIcon className="size-5" />} />
      <PhoneTab
        label={chats.copy.tabs.chats}
        icon={<ChatsIcon className="size-5" />}
        selected={selected === `chats`}
        dot="bg-yellow-400"
      />
      <PhoneTab label="Acme" icon={<TeamAvatar name="Acme" size={20} />} selected={selected === `team`} />
    </PhoneTabBar>
  )
}

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
      state: `n/a`,
      note: `the IDE has no phone shell (styleguide twin phone_tab_bar.rs)`,
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
  island: () => (
    <div className="flex flex-col items-start gap-4 rounded-xl bg-background p-4">
      <Bar selected="chats" />
      <Bar selected="team" />
    </div>
  ),
}
