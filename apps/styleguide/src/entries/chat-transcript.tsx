import fixture from "@exp/domain-contract/fixtures/chats.json"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { SurfaceCard, ThreadConversation, TooltipProvider } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// Thread 58cb32df (fixtures/chats.json TRANSCRIPT + SURFACE CARD ×4): the
// chat's transcript, the REAL `ThreadConversation` in both layouts: phones
// paint chat BUBBLES (the person right in `UserMessageBubble` `chat`, the
// agent's text left in THE `ChatBubble`, each with its time), web wide keeps
// the transcript. A person's `/compact` / `/new` = THE `TranscriptDivider`
// pill; an agent's preview = THE `SurfaceCard` (the plan card's renderer in
// its own glass frame); a plan the agent replaced paints collapsed to
// `copy.superseded`. The enter animation replays from the checkbox (the
// frame's fade + rise + scale; the app also staggers the root's children and
// settles the primary hairline, `surfaceCard.enter`).

const COPY = fixture.copy

const paragraph = (text: string) => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

/** An agent's inline preview (the core catalog), its two presses `reply`. */
const LAUNCH_SURFACE = {
  root: {
    id: `root`,
    component: `Stack`,
    props: { gap: `md` },
    children: [
      {
        id: `stat`,
        component: `StatTile`,
        props: { label: `Cold start, Pixel 6a`, value: `740 ms`, delta: `−47 %`, tone: `success`, trend: [1400, 1310, 1180, 980, 820, 740] },
        children: [],
      },
      {
        id: `actions`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `sm` },
        children: [
          {
            id: `ship`,
            component: `Button`,
            props: { label: `Ship snapshot` },
            children: [],
            on: { press: { event: { name: fixture.surfaceCard.replyEvent, context: { text: `Ship the snapshot` } } } },
          },
          {
            id: `keep`,
            component: `Button`,
            props: { label: `Keep profiling`, variant: `secondary` },
            children: [],
            on: { press: { event: { name: fixture.surfaceCard.replyEvent, context: { text: `Keep profiling` } } } },
          },
        ],
      },
    ],
  },
  data: {},
}

const ROWS: ThreadRow[] = [
  {
    type: `message`,
    id: `m1`,
    author: `user`,
    authorId: `u-dana`,
    body: `Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it.`,
    queued: false,
    authorSessionId: null,
  },
  {
    type: `plan`,
    id: `p-old`,
    sessionId: `r`,
    plan: `# Plan\nProfile first`,
    questions: [],
    surface: null,
    decision: `superseded`,
    fresh: false,
    answers: null,
    feedback: null,
    surfaceData: null,
  },
  {
    type: `message`,
    id: `m2`,
    author: `agent`,
    authorId: null,
    body: `Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:`,
    queued: false,
    authorSessionId: null,
  },
  { type: `surface`, id: `s1`, sessionId: `r`, title: `Launch path`, surface: LAUNCH_SURFACE },
  { type: `divider`, id: `d1`, kind: `compact`, at: `2026-10-23T18:20:00.000Z` },
  {
    type: `message`,
    id: `m3`,
    author: `user`,
    authorId: `u-dana`,
    body: `Anything new overnight?`,
    queued: false,
    authorSessionId: null,
  },
  {
    type: `message`,
    id: `m4`,
    author: `agent`,
    authorId: null,
    body: `Three reports, all about the iPad sign-in. I linked them to #APP-19.`,
    queued: false,
    authorSessionId: null,
  },
  { type: `divider`, id: `d2`, kind: `new`, at: `2026-10-24T09:30:00.000Z` },
]

const ENTER_TEXT = `${fixture.surfaceCard.enter.frame.fromOpacity}, ${fixture.surfaceCard.enter.frame.fromTranslateY}px lower at ${fixture.surfaceCard.enter.frame.fromScale} over \`${fixture.surfaceCard.enter.frame.duration}\`, the primary hairline settling, then the root's children ${fixture.surfaceCard.enter.children.staggerMs} ms apart`

const TIMES: Record<string, string> = { m1: `21:02`, m2: `21:14`, m3: `09:02`, m4: `09:03` }

function Transcript({ layout }: { layout: `transcript` | `bubbles` }) {
  return (
    <ThreadConversation
      rows={ROWS}
      layout={layout}
      renderText={paragraph}
      attachmentSrc={() => ``}
      messageCaption={() => `Dana Reyes · 21:02`}
      messageTime={(row) => TIMES[row.id] ?? null}
      onSurfaceReply={() => {}}
      arrived={() => false}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `chat-transcript`,
  section: `special`,
  owner: `EXP-1373`,
  title: `Chat transcript`,
  blurb: `A chat's transcript ×4 (\`buildSessionThread\` rows, fixtures/chats.json TRANSCRIPT). Phones paint CHAT BUBBLES: the person's on the right (max 80%, its time inside), the agent's text on the left in a bg-card bubble (max 94%, the top-left corner 6px, its time bottom-right), cards in the agent's column; a new live bubble rises in (opacity + 6px at standard / decelerate). Web wide and the IDE keep the transcript. A person's \`/new\` or \`/compact\` is never a bubble: it is the centred glass pill "${COPY.dividers.compact} · 18:20" / "${COPY.dividers.new} · 09:30". An agent's inline Exponential UI preview (card \`surface\`, MCP \`exponential_session_self {op: "show"}\`) paints through the plan card's renderer (theme exponential, dark) in its own rounded glass frame; a press whose event is \`${fixture.surfaceCard.replyEvent}\` with a \`context.text\` sends that text as the person's next message. A surface that ARRIVES while the chat is open plays its enter once (fade from ${ENTER_TEXT}); reduce motion = an opacity fade only. A waiting plan or question card is always inline at its place with its actions (besides the dock); a plan the agent replaced reads "${COPY.superseded}", collapsed.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadConversation layout="bubbles" / SurfaceCard / TranscriptDivider / ChatBubble`,
      file: `packages/ui/src/surface-card.tsx`,
      note: `session-thread-view.tsx paints the rows; the raw session view (agent-session.tsx) the same divider, surface and phone bubble`,
    },
    desktop: { state: `ok`, symbol: `surface_card`, file: `apps/desktop/crates/ui/src/surface_card.rs`, note: `twin: styleguide/entries/chat_transcript.rs` },
    ios: { state: `ok`, symbol: `SurfaceCard / TranscriptDivider / ChatBubble`, file: `apps/ios/ExpUI/Sources` },
    android: { state: `ok`, symbol: `SurfaceCard / TranscriptDivider / ChatBubble`, file: `apps/android/app/src/main/java/com/exponential/app/ui/thread` },
  },
  island: () => (
    <TooltipProvider>
      <div className="flex flex-wrap items-start gap-10">
        <div className="w-[24rem] rounded-3xl border border-glass-stroke-card bg-background p-4">
          <div className="mb-3 text-xs text-muted-foreground">Phone (bubbles)</div>
          <Transcript layout="bubbles" />
        </div>
        <div className="w-[40rem]">
          <div className="mb-3 text-xs text-muted-foreground">Web wide (transcript)</div>
          <Transcript layout="transcript" />
        </div>
        <div className="w-[24rem]">
          <input id="sg-surface-enter" type="checkbox" className="peer sr-only" />
          <label
            htmlFor="sg-surface-enter"
            className="mb-3 inline-flex cursor-pointer items-center rounded-full border border-glass-stroke-card bg-glass-row px-3 py-1 text-xs text-foreground peer-checked:bg-glass-active"
          >
            Replay the enter
          </label>
          <SurfaceCard
            surface={LAUNCH_SURFACE}
            title="Launch path"
            testKey="sg-enter"
            className="peer-checked:motion-enter peer-checked:slide-in-from-bottom-[12px] peer-checked:zoom-in-[0.98] peer-checked:[--tw-duration:var(--motion-duration-slow)]"
          />
        </div>
      </div>
    </TooltipProvider>
  ),
}

