import chats from "@exp/domain-contract/fixtures/chats.json"
import { ChatListSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (#EXP-1373, `chats.json`): THE phone Chats screen's pieces,
// `@exp/ui` `ChatListSpecimen` (the real components the web-mobile page
// mounts, fed the fixture's `listCases[0]` + `bubbleCases[0]`); the app's
// `?dev=chat-list` capture renders the same.

export const entry: StyleguideEntry = {
  id: `chat-list`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat list`,
  blurb: `THE Chats list ×4 (phones; since #EXP-1380 the wide web and the IDE paint the same bubbles, chips and rows in one column, without the phone headline): on a phone the large "${chats.copy.title}" headline with the search and inbox circles (a sky dot while a notification is unread), THE story row (#EXP-1461: only story rings of the last 24 h at ${chats.shell.bubble}px, an agent bubble only while its agent has one, then the chat rings; nobody has one = no row; scrolls sideways), the filter chips (All · Needs you N · ${chats.copy.filters.running} N · Unread N, select Pills), then one flat row per root chat of the ACTIVE team by activity: the ${chats.shell.leadDisc}px lead disc (the brand mark with the amber badge, the working mark, the PR glyph open emerald / merged violet, else the bare mark), the title (semibold while unread) and the time (amber while it needs you), the two-line muted preview, the row word and the unread badge. S-3: a DONE chat (every PR merged, nothing works or needs you) stays in the list, the whole row at ${chats.shell.dimOpacity} opacity. A row hairline runs under the text column only. Right-click / long-press = the chat menu (\`menuProps('thread', id)\`), a bubble's = the agent menu.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ChatsHeader / AgentBubbles / ChatFilterChips / ChatRow / ChatList`,
      file: `packages/ui/src/chat-list.tsx`,
      note: `the phone branch of routes/t/$teamSlug/threads/index.tsx (model: hooks/use-chat-list.ts)`,
    },
    desktop: {
      state: `ok`,
      symbol: `chat_list::{agent_bubbles_row, chat_filter_chips, chat_row, chat_lead_disc}`,
      file: `apps/desktop/crates/ui/src/chat_list.rs`,
      note: `the IDE Chats page (threads_view.rs) and the styleguide twin (styleguide/entries/chat_list.rs) share it`,
    },
    ios: {
      state: `ok`,
      symbol: `ChatRow / AgentBubbles`,
      file: `apps/ios/ExpUI/Sources/ChatList.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ChatRow / AgentBubbles`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/ChatComponents.kt`,
    },
  },
  island: () => <ChatListSpecimen />,
}
