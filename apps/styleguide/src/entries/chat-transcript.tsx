import fixture from "@exp/domain-contract/fixtures/chats.json"
import { ChatTranscriptSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (fixtures/chats.json TRANSCRIPT + SURFACE CARD ×4): the
// chat's transcript in both layouts, `@exp/ui` `ChatTranscriptSpecimen` (the
// app's `?dev=chat-transcript` capture renders the same).

const COPY = fixture.copy

const ENTER_TEXT = `${fixture.surfaceCard.enter.frame.fromOpacity}, ${fixture.surfaceCard.enter.frame.fromTranslateY}px lower at ${fixture.surfaceCard.enter.frame.fromScale} over \`${fixture.surfaceCard.enter.frame.duration}\`, the primary hairline settling, then the root's children ${fixture.surfaceCard.enter.children.staggerMs} ms apart`

export const entry: StyleguideEntry = {
  id: `chat-transcript`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat transcript`,
  blurb: `A chat's transcript ×4 (\`buildSessionThread\` rows, fixtures/chats.json TRANSCRIPT). Phones paint CHAT BUBBLES: the person's on the right (max 80%, its time inside), the agent's text on the left on THE card surface (GlassCard's fill + hairline, max 94%, the top-left corner 6px, its time bottom-right), cards in the agent's column; a new live bubble rises in (opacity + 6px at standard / decelerate). Web wide and the IDE keep the transcript. A person's \`/new\` or \`/compact\` is never a bubble: it is the centred glass pill "${COPY.dividers.compact} · 18:20" / "${COPY.dividers.new} · 09:30". An agent's inline Exponential UI preview (card \`surface\`, MCP \`exponential_session_self {op: "show"}\`) paints through the plan card's renderer (theme exponential, dark) in its own rounded glass frame; a press whose event is \`${fixture.surfaceCard.replyEvent}\` with a \`context.text\` sends that text as the person's next message. A surface that ARRIVES while the chat is open plays its enter once (fade from ${ENTER_TEXT}); reduce motion = an opacity fade only. A waiting plan or question card is always inline at its place with its actions (besides the dock); a plan the agent replaced reads "${COPY.superseded}", collapsed.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SurfaceCard / TranscriptDivider / ChatBubble`,
      file: `packages/ui/src/surface-card.tsx`,
      note: `session-thread-view.tsx ThreadConversation layout="bubbles" paints the rows; agent-session.tsx the same parts`,
    },
    desktop: { state: `ok`, symbol: `sync_rows / divider_pill`, file: `apps/desktop/crates/ui/src/surface_card.rs`, note: `twin: styleguide/entries/chat_transcript.rs` },
    ios: { state: `ok`, symbol: `SurfaceCardFrame / TranscriptDividerPill / AgentChatBubble`, file: `apps/ios/ExpUI/Sources/ChatTranscript.swift` },
    android: { state: `ok`, symbol: `ChatBubble`, file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadParts.kt`, note: `TranscriptDivider: ui/components/ChatComponents.kt; SurfaceCard: ui/thread/SurfaceCard.kt` },
  },
  island: () => <ChatTranscriptSpecimen />,
}
