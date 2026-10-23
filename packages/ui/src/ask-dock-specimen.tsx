import type { ReactNode } from "react"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { dockRows, type DockSource } from "@exp/domain-contract/dock"
import { DockCard, DockPile, DockSheetSpecimenFrame } from "./dock"
import { ThreadPlanSheet } from "./session-thread-view"

// N-12: THE Dock specimen (fixtures/dock.json + ask-dock.json ×4, the
// CANONICAL one: the IDE, iOS and Android specimens draw exactly these
// rows): the DockCards at rest in their four states: one waiting plan (a
// single card), the pile (the queued messages, THE Queue, the suggested
// tasks, a plan, a question: the front card whole, the others peeking out
// above it), the pile fanned, and a plan card's sheet (the real `ThreadPlanSheet` in the sheet's static frame at
// rest on page 0: the pinned header, the plan as the body, Next in the footer). The
// styleguide entry and the app's `?dev=ask-dock` capture render it; the
// natives' specimens draw the same rows.

const LONG_PLAN = [
  `## Plan: one Dock ×4`,
  ``,
  `1. Add \`fixtures/dock.json\` + the TS model, mirrored ×4.`,
  `2. Float the waiting cards above the composer; the transcript is the one scroll.`,
  `3. Pile several cards like the toasts; a tap opens one as a sheet.`,
  `4. Share THE plan options: Yes · Yes, and start with a fresh context · No.`,
  `5. Styleguide entry + scoped shots on every platform.`,
  `6. Open the PR with the Guide.`,
].join(`\n`)

const plan = (id: string, text: string, sessionId: string, questions: ThreadRow[] = []): ThreadRow => ({
  type: `plan`,
  id,
  sessionId,
  plan: text,
  questions: questions as never,
  surface: null,
  decision: null,
  fresh: false,
  answers: null,
  feedback: null,
  surfaceData: null,
})

const QUESTIONS = [
  {
    id: `q-scope`,
    prompt: `Which cards move into the Dock first?`,
    multi: false,
    custom: true,
    options: [
      { label: `Plan and question cards`, description: `The rest follows as its own issue`, attachmentId: null },
      { label: `Everything`, description: `Plan, question, tasks, queued messages, banners`, attachmentId: null },
    ],
  },
  {
    id: `q-eui`,
    prompt: `Should DockCard also go into Exponential UI?`,
    multi: false,
    custom: false,
    options: [
      { label: `Yes, as DockCard`, description: null, attachmentId: null },
      { label: `Styleguide only for now`, description: null, attachmentId: null },
    ],
  },
]

const RICH_PLAN = plan(`p1`, LONG_PLAN, `a`, QUESTIONS as never)

const NAMES: Record<string, string> = { a: `Ask dock`, b: `Thread list`, c: `Dock polish` }

const ASKS: DockSource[] = [
  { kind: `plan`, id: `p1`, text: LONG_PLAN, sessionName: NAMES.a!, questions: 2, createdAt: 10 },
  { kind: `question`, id: `q1`, text: `Ship both fixes in one PR, or split them?`, sessionName: NAMES.c!, createdAt: 30 },
]
const STATE: DockSource[] = [
  { kind: `queued`, messages: [{ id: `m1`, text: `also check the watchdog once the root is idle` }, { id: `m2`, text: `then open the PR` }] },
  {
    kind: `tasks`,
    queue: {
      todo: {
        title: `3 to do`,
        items: [
          { label: `Measuring on the Galaxy A54`, description: `2m 35s`, state: `running` },
          { label: `Rerun the web captures`, description: null, state: `pending` },
          { label: `Open the PR with the numbers`, description: null, state: `pending` },
        ],
      },
      completed: { title: `2 completed` },
    },
  },
  {
    kind: `suggestions`,
    id: `m4`,
    label: `3 suggested tasks`,
    caption: `1 started`,
    title: `Lazy-load the markdown editor`,
    open: 2,
  },
]

/** A specimen-sized stand-in for the app's GFM renderer: a heading line
 *  reads medium, code ticks drop, everything else is a line of its own. */
const text = (body: string) => (
  <div className="flex flex-col gap-0.5">
    {body.split(`\n`).map((line, index) =>
      line.trim() === `` ? null : line.startsWith(`#`) ? (
        <p key={index} className="mb-1 font-medium">
          {line.replace(/^#+\s*/, ``)}
        </p>
      ) : (
        <p key={index}>{line.replaceAll("`", ``)}</p>
      )
    )}
  </div>
)
const noop = () => {}

function Case({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      {children}
    </div>
  )
}

/** The pile at rest: `DockPile` with the fixture's rows, piled or fanned. */
function PileCase({ expanded }: { expanded: boolean }) {
  const rows = dockRows([...STATE, ...ASKS])
  return (
    <div className="flex flex-col gap-1.5">
      <DockPile rows={rows} expanded={expanded} onExpandedChange={noop} onOpen={noop} />
    </div>
  )
}

export function AskDockSpecimen() {
  const single = dockRows([ASKS[0]!])
  return (
    <div data-testid="ask-dock-specimen" className="flex max-w-2xl flex-col gap-6 text-sm">
      <Case label="One waiting plan">
        <DockCard row={single[0]!} onOpen={noop} />
      </Case>
      <Case label="A plan, a question, the suggested tasks, the Queue and the queued messages, piled">
        <PileCase expanded={false} />
      </Case>
      <Case label="The pile fanned (hover or a tap)">
        <PileCase expanded />
      </Case>
      <Case label="The plan's sheet">
        <DockSheetSpecimenFrame row={single[0]!} height={420}>
          <ThreadPlanSheet row={RICH_PLAN as never} renderText={text} attachmentSrc={(id) => id} busy={false} onDecide={noop} />
        </DockSheetSpecimenFrame>
      </Case>
    </div>
  )
}
