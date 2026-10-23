import chats from "@exp/domain-contract/fixtures/chats.json"
import type { AgentBubble, ChatFilter, ChatRow as ChatRowModel } from "@exp/domain-contract/chats"
import { AgentBubbles, ChatFilterChips, ChatList, ChatsHeader } from "@exp/ui"
import { useState } from "react"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (#EXP-1373, `chats.json`): THE phone Chats screen's pieces,
// the real `@exp/ui` components the web-mobile page mounts, fed the
// fixture's own expected model: `listCases[0]` (one flat list by activity,
// leads working / needs you / open PR / merged, the unread badge, the
// `Yesterday` label) and `bubbleCases[0]` (amber = needs you, white =
// unread, a hairline = read). The chips filter the same rows locally.

const LIST = chats.listCases[0]!.expected
const ROWS = LIST.rows as ChatRowModel[]
const BUBBLES = chats.bubbleCases[0]!.expected as AgentBubble[]

function ChatsSpecimen() {
  const [filter, setFilter] = useState<ChatFilter>(`all`)
  const rows = ROWS.filter(
    (row) =>
      filter === `all` ||
      (filter === `needs_you` && row.group === `needs_you`) ||
      (filter === `running` && row.group === `running`) ||
      (filter === `unread` && row.unreadCount > 0)
  )
  return (
    <div className="flex w-[390px] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4">
      <ChatsHeader inboxUnread />
      <AgentBubbles bubbles={BUBBLES} />
      <ChatFilterChips value={filter} counts={LIST.counts} onChange={setFilter} />
      <ChatList rows={rows} agentOf={() => `claude`} />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `chat-list`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat list`,
  blurb: `THE phone Chats screen ×3 (iOS, Android, web-mobile; the IDE has no phone list): the large "${chats.copy.title}" headline with the search and inbox circles (a sky dot while a notification is unread), the agent BUBBLES (${chats.shell.bubble}px circles, a ${chats.shell.bubbleRing}px ring around a background gap: amber = needs you, white = an unread reply, a hairline = read; scrolls sideways), the filter chips (All · Needs you N · Running N · Unread N, select Pills), then one flat row per root chat of the ACTIVE team by activity: the ${chats.shell.leadDisc}px lead disc (the brand mark with the amber badge, the working mark, the PR glyph open emerald / merged violet, else the bare mark), the title (semibold while unread) and the time (amber while it needs you), the two-line muted preview, the row word and the unread badge. A row hairline runs under the text column only. Right-click / long-press = the chat menu (\`menuProps('thread', id)\`), a bubble's = the agent menu.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ChatsHeader / AgentBubbles / ChatFilterChips / ChatRow / ChatList`,
      file: `packages/ui/src/chat-list.tsx`,
      note: `the phone branch of routes/t/$teamSlug/threads/index.tsx (model: hooks/use-chat-list.ts)`,
    },
    desktop: {
      state: `n/a`,
      note: `the IDE has no phone list; its Chats page keeps the bands (styleguide twin chat_list.rs)`,
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
  island: () => <ChatsSpecimen />,
}
