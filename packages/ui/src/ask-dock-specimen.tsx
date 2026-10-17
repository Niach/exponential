import type { ReactNode } from "react"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { ThreadAskDock } from "./session-thread-view"

// THE ask dock specimen (fixture `ask-dock.json` ×4): the REAL
// `ThreadAskDock` in its three states, one waiting plan (the big card), two
// plans + a question (the stack) and one card focused out of the stack (the
// back arrow). The styleguide entry and the app's `?dev=ask-dock` capture
// render it; the natives' specimens draw the same rows.

const LONG_PLAN = [
  `## Plan: one ask dock ×4`,
  ``,
  `1. Add \`fixtures/ask-dock.json\` + the TS model, mirrored ×4.`,
  `2. Move waiting cards out of the transcript into the composer's slot.`,
  `3. Stack several cards; a tap grows one into the big card.`,
  `4. Share THE plan options: Yes · Yes, and start with a fresh context · No.`,
  `5. Styleguide entry + scoped shots on every platform.`,
  `6. Open the PR with the Guide.`,
].join(`\n`)

const plan = (id: string, text: string, sessionId: string): ThreadRow => ({
  type: `plan`,
  id,
  sessionId,
  plan: text,
  questions: [],
  surface: null,
  decision: null,
  fresh: false,
  answers: null,
  feedback: null,
})

const STACK: ThreadRow[] = [
  plan(`p1`, LONG_PLAN, `a`),
  plan(`p2`, `1. Group the thread list by state\n2. Drop the per-row counters`, `b`),
  {
    type: `question`,
    id: `q1`,
    sessionId: `c`,
    question: `Ship both fixes in one PR, or split them?`,
    options: [
      { label: `One PR`, description: null, attachmentId: null },
      { label: `Split them`, description: null, attachmentId: null },
    ],
    custom: true,
    multi: false,
    surface: null,
    answer: null,
    answers: null,
    answered: false,
  },
]

const NAMES: Record<string, string> = { a: `Ask dock`, b: `Thread list`, c: `Dock polish` }
const name = (id: string) => NAMES[id] ?? null
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

export function AskDockSpecimen() {
  return (
    <div data-testid="ask-dock-specimen" className="flex max-w-2xl flex-col gap-6 text-sm">
      <Case label="One waiting plan">
        <ThreadAskDock rows={[STACK[0]!]} sessionName={name} renderText={text} onAnswer={noop} onDecide={noop} />
      </Case>
      <Case label="Two plans and a question">
        <ThreadAskDock rows={STACK} sessionName={name} renderText={text} onAnswer={noop} onDecide={noop} />
      </Case>
      <Case label="Focused out of the stack">
        <ThreadAskDock
          rows={STACK}
          sessionName={name}
          renderText={text}
          onAnswer={noop}
          onDecide={noop}
          initialFocusedId="p2"
        />
      </Case>
    </div>
  )
}
