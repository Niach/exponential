/* The /ui/ home demo's screen: the Exponential app's own phone chat (the
   REAL `@exp/ui` pieces: THE phone page header, `ThreadConversation` in its
   bubbles layout with an agent's `SurfaceCard`, THE floating composer
   capsule) answering a prompt with an Exponential UI surface that is
   generated on the fly, node by node. `scene(step)` is the whole film: the
   prerendered island shows its LAST frame (scripts/ui-demos.tsx), the
   client replays every frame inside the same shadow root (PhoneDemo.tsx).
   The surface is fixtures/demo-surface.json (= specimen exponential-ui-demo). */
import type { ComponentProps, ReactNode } from "react"
import { DETAIL_EDGE_BOTTOM_CLASS, MobileWorkCapsule, PageHeader, ThreadConversation, TooltipProvider } from "@exp/ui"
import demo from "@exponential_app/ui/fixtures/demo-surface.json"

type Row = ComponentProps<typeof ThreadConversation>[`rows`][number]
type Rows = Row[]
type Node = { id: string; style?: Record<string, unknown>; children?: Node[]; [key: string]: unknown }

export const PROMPT = `How did last night's release go?`
const ANSWER = `Clean. Three platforms shipped, one crash, already fixed. Here it is:`
const AGENT = `Assistant`
const PLACEHOLDER = `Message ${AGENT}…`

/* The surface without its page padding (the card gives it its inset). */
const { padding: _padding, ...rootStyle } = (demo as unknown as Node).style ?? {}
const SURFACE: Node = { ...(demo as unknown as Node), style: rootStyle }

/** The tree cut to its first `n` nodes in reading order: what an agent has
 *  streamed so far. */
function prefix(tree: Node, n: number): Node {
  let left = n
  const walk = (node: Node): Node | null => {
    if (left <= 0) return null
    left -= 1
    const children = (node.children ?? []).map(walk).filter((c): c is Node => c !== null)
    return { ...node, children }
  }
  return walk(tree) ?? { ...tree, children: [] }
}

function count(tree: Node): number {
  return 1 + (tree.children ?? []).reduce((sum, c) => sum + count(c), 0)
}

const NODES = count(SURFACE)
const WORDS = ANSWER.split(` `)

/* The film's frames: the prompt typed into the capsule, sent, the answer's
   words, then the surface's nodes. */
const TYPE = PROMPT.length
const SEND = TYPE + 1
const WORDS_END = SEND + WORDS.length
export const LAST_STEP = WORDS_END + NODES

/** Milliseconds a frame stays: typing is quick, the surface lands in chunks. */
export function frameMs(step: number): number {
  if (step < TYPE) return 38
  if (step === TYPE) return 420
  if (step < WORDS_END) return step === SEND ? 650 : 70
  return 110
}

const paragraph = (text: string): ReactNode => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

export function scene(step: number): { rows: Rows; draft: string; streaming: boolean } {
  const rows: Rows = []
  if (step > TYPE) {
    rows.push({ type: `message`, id: `prompt`, author: `user`, authorId: `you`, body: PROMPT, queued: false, authorSessionId: null, xref: null })
  }
  const words = Math.min(WORDS.length, Math.max(0, step - SEND))
  if (words > 0) {
    rows.push({ type: `message`, id: `answer`, author: `agent`, authorId: null, body: WORDS.slice(0, words).join(` `), queued: false, authorSessionId: null, xref: null })
  }
  const nodes = Math.max(0, step - WORDS_END)
  if (nodes > 0) {
    /* A new row id per frame remounts the card on the streamed tree; the
       last frame's id is the stable one. */
    const id = nodes >= NODES ? `release` : `release-${nodes}`
    rows.push({ type: `surface`, id, sessionId: `assistant`, title: `Release 0.18.84`, surface: { root: prefix(SURFACE, nodes), data: {} } })
  }
  return { rows, draft: step < TYPE ? PROMPT.slice(0, step) : step === TYPE ? PROMPT : ``, streaming: step > TYPE && step < LAST_STEP }
}

export function PhoneChat({ step = LAST_STEP }: { step?: number }) {
  const { rows, draft } = scene(step)
  return (
    <TooltipProvider>
      <div className="relative flex h-[820px] flex-col bg-background pt-[54px]">
        <PageHeader title={AGENT} className="px-4 pt-2 pb-3" />
        <div className="flex min-h-0 flex-1 flex-col justify-end overflow-hidden px-3 pb-20" aria-live="polite">
          <ThreadConversation
            rows={rows}
            layout="bubbles"
            renderText={paragraph}
            attachmentSrc={() => ``}
            messageCaption={() => `You`}
            messageTime={(row) => (row.id === `prompt` ? `09:41` : row.id === `answer` ? `09:41` : null)}
            onSurfaceReply={() => {}}
            arrived={(id) => id === `release-1`}
          />
        </div>
        {/* THE floating bar's geometry (MobileWorkBar, which pins itself to
            the viewport): the 20px inset, the bottom edge fade, the capsule. */}
        <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex items-end px-5 pb-4">
          <span aria-hidden className={DETAIL_EDGE_BOTTOM_CLASS} />
          <MobileWorkCapsule tabIndex={-1}>{draft ? <span className="truncate text-foreground">{draft}</span> : PLACEHOLDER}</MobileWorkCapsule>
        </div>
      </div>
    </TooltipProvider>
  )
}
