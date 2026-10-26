/* The home hero's screen (NEW-15): a team agent's chat on the phone, from the
   pieces the agent channel draws (apps/web components/thread/agent-channel.tsx,
   bubbles layout): THE phone detail header (back · agent disc + `Agent` · …),
   `ThreadConversation` with the person's message, the agent's bubble, its
   Exponential UI surface generated on the fly node by node, the person's
   tap on its button and the session that starts, then THE floating
   composer capsule. `scene(step)` is the whole film, as PhoneChat's on /ui/: the
   prerendered island shows the LAST frame, the page replays every frame.
   The header is the app's own (MobileDetailHeader + AgentHeaderTitle, the
   agent route's menu drawn as its round ghost button). */
import type { ComponentProps, ReactNode } from "react"
import {
  AgentHeaderTitle,
  Button,
  DETAIL_EDGE_BOTTOM_CLASS,
  HEADER_BUTTON_CLASS,
  MobileDetailHeader,
  MobileWorkCapsule,
  ThreadConversation,
  TooltipProvider,
  conceptIcon,
} from "@exp/ui"

type Rows = ComponentProps<typeof ThreadConversation>[`rows`][number][]
type Node = { id: string; component: string; props?: Record<string, unknown>; style?: Record<string, unknown>; children?: Node[] }

const MoreIcon = conceptIcon(`ui-more`)

const AGENT = { name: `Sloppy`, icon: `bot`, color: `#f97316` }
export const PROMPT = `How did the week go?`
const ANSWER = `Good week. One thing is stuck:`
const TAP = `Fix the blocker`

/* The answer's surface, in the nested authoring form (as fixtures/demo-surface.json):
   flat on purpose, ONE chart, two rows and the two actions, no cards inside the card. */
const SURFACE: Node = {
  id: `week`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.lg` },
  children: [
    {
      id: `week-chart`,
      component: `Chart`,
      props: {
        kind: `bar`,
        height: 132,
        showLegend: false,
        showGrid: false,
        categories: [`Mon`, `Tue`, `Wed`, `Thu`, `Fri`],
        series: [{ name: `Shipped`, values: [2, 3, 1, 4, 2] }],
      },
    },
    { id: `week-shipped`, component: `Row`, props: { title: `Shipped`, value: `12` } },
    { id: `week-blocked`, component: `Row`, props: { title: `Blocked`, identifier: `APP-9`, subtitle: `Uploads fail on Android`, value: `2 days` } },
    {
      id: `week-actions`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, justify: `end` },
      children: [
        { id: `week-board`, component: `Button`, props: { label: `Open board`, variant: `outline` } },
        { id: `week-fix`, component: `Button`, props: { label: TAP, variant: `default` } },
      ],
    },
  ],
}

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

const count = (tree: Node): number => 1 + (tree.children ?? []).reduce((sum, c) => sum + count(c), 0)

const NODES = count(SURFACE)
const WORDS = ANSWER.split(` `)
const TYPE = PROMPT.length
const SEND = TYPE + 1
const WORDS_END = SEND + WORDS.length
const SURFACE_END = WORDS_END + NODES
const TAPPED = SURFACE_END + 1
export const LAST_STEP = TAPPED + 1

/** Milliseconds a frame stays: typing is quick, the surface lands in chunks,
 *  the tap and the session get a beat each. */
export function frameMs(step: number): number {
  if (step < TYPE) return 38
  if (step === TYPE) return 420
  if (step < WORDS_END) return step === SEND ? 650 : 70
  if (step < SURFACE_END) return 160
  return 1200
}

export function scene(step: number): { rows: Rows; draft: string } {
  const rows: Rows = []
  if (step > TYPE) rows.push({ type: `message`, id: `ask`, author: `user`, authorId: `you`, body: PROMPT, queued: false, authorSessionId: null })
  const words = Math.min(WORDS.length, Math.max(0, step - SEND))
  if (words > 0) rows.push({ type: `message`, id: `answer`, author: `agent`, authorId: null, body: WORDS.slice(0, words).join(` `), queued: false, authorSessionId: null })
  const nodes = Math.min(NODES, Math.max(0, step - WORDS_END))
  if (nodes > 0) {
    /* A new row id per frame remounts the card on the streamed tree; the
       last frame's id is the stable one. */
    rows.push({ type: `surface`, id: nodes >= NODES ? `week` : `week-${nodes}`, sessionId: `sloppy`, title: `Shipped this week`, surface: { root: prefix(SURFACE, nodes), data: {} } })
  }
  if (step >= TAPPED) rows.push({ type: `message`, id: `tap`, author: `user`, authorId: `you`, body: TAP, queued: false, authorSessionId: null })
  if (step >= LAST_STEP) {
    rows.push({
      type: `session`,
      id: `fix`,
      card: {
        sessionId: `fix`,
        icon: `coding-running`,
        name: `APP-9 Uploads fail on Android`,
        kindWord: `Working`,
        state: `working`,
        tone: `foreground`,
        elapsed: `8s`,
        running: true,
        testId: `session-card`,
      },
    })
  }
  return { rows, draft: step < TYPE ? PROMPT.slice(0, step) : step === TYPE ? PROMPT : `` }
}

const paragraph = (text: string): ReactNode => <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>

export function AgentChat({ step = LAST_STEP }: { step?: number }) {
  const { rows, draft } = scene(step)
  return (
    <TooltipProvider>
      <div className="relative flex h-[820px] flex-col bg-background pt-[54px]">
        <MobileDetailHeader
          title={<AgentHeaderTitle name={AGENT.name} icon={AGENT.icon} color={AGENT.color} />}
          onBack={() => {}}
          menu={
            <Button variant="ghost" size="icon" className={HEADER_BUTTON_CLASS} aria-label="More">
              <MoreIcon className="size-4" />
            </Button>
          }
        />
        <div className="flex min-h-0 flex-1 flex-col justify-end overflow-hidden px-3 pb-20" aria-live="polite">
          <ThreadConversation
            rows={rows}
            layout="bubbles"
            renderText={paragraph}
            attachmentSrc={() => ``}
            messageCaption={() => `You`}
            messageTime={() => `09:41`}
            onSurfaceReply={() => {}}
          />
        </div>
        <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex items-end px-5 pb-4">
          <span aria-hidden className={DETAIL_EDGE_BOTTOM_CLASS} />
          <MobileWorkCapsule tabIndex={-1}>{draft ? <span className="truncate text-foreground">{draft}</span> : `Message ${AGENT.name}`}</MobileWorkCapsule>
        </div>
      </div>
    </TooltipProvider>
  )
}
