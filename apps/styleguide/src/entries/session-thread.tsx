import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread } from "@exp/domain-contract/session-tree"
import { ThreadConversation, userMessageCaption } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: a thread's CONVERSATION, the REAL `ThreadConversation`
// over `buildSessionThread()` run on the contract's own cases
// (`session-thread.json`, ×4): the root's session card first, a person's
// bubble right-aligned (`queued` until the device consumed it), the agent's
// text, a child's session card, an event line, a question card answered in
// the card.

type Input = Parameters<typeof buildSessionThread>[0]

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

const AT = Date.UTC(2026, 9, 13, 19, 56)

function Case({ index }: { index: number }) {
  const found = fixture.cases[index]
  if (!found) return null
  const view = buildSessionThread(found.input as unknown as Input)
  return (
    <ThreadConversation
      rows={view.rows}
      renderText={paragraph}
      attachmentSrc={() => SHOT}
      messageCaption={(row) => {
        const caption = userMessageCaption({ name: `Alex Carter`, at: AT })
        return row.queued ? `${caption} · queued` : caption
      }}
      onOpenSession={() => {}}
      onAnswer={() => {}}
      onDecide={() => {}}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `session-thread`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Conversation`,
  blurb: `A thread's Conversation tab (a thread IS a root session): the root's own session card first, then the ROOT's messages oldest first. A person's message is the right-aligned bubble (\`<name> · <time>\` under it, \`queued\` until the device consumed it); the agent's reply is markdown at the reading width; a system line is muted. A \`session\` / \`thread_started\` card is THE session card (the Work card's header row) opening that session; a question card offers its options IN the card, a plan card Approve / Reject; a subagent is a muted row, a picture the image, an event one muted line (\`Linked EXP-12\`, \`Opened PR #45\`). Children's own messages never inline. No captions, no Show work, no right sidebar. Model = \`buildSessionThread()\` (fixture \`session-thread.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadConversation`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `page: apps/web/src/routes/t/$teamSlug/threads/$threadId.tsx; model: domain-contract session-tree.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `ThreadScreenView`,
      file: `apps/desktop/crates/ui/src/thread_screen.rs`,
      note: `model: crates/domain/src/session_tree.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/ios/Exponential/UI/Thread/ThreadScreen.swift`,
      note: `model: ExpCore SessionTree.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadScreen.kt`,
      note: `model: domain/SessionTree.kt`,
    },
  },
  island: () => (
    <div className="flex max-w-2xl flex-col gap-10">
      <Case index={0} />
      <Case index={1} />
    </div>
  ),
}
