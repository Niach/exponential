import { useMemo } from "react"
import { PLAN_PAGE_SPECIMEN, planRowLabel } from "@exp/domain-contract/plan-page"
import { STORY_TAB } from "@exp/domain-contract/stories"
import { THREAD_COPY, threadAskRow, type TreeMessage } from "@exp/domain-contract/session-tree"
import { cn } from "./cn"
import { PlanPage } from "./plan-page"
import { ThreadTabs } from "./session-thread-view"
import { useSpecimenAttachmentSrc } from "./specimen-samples"

// #EXP-1493: THE plan page's styleguide specimen (plan-page.json
// `specimen`), wide and phone, drawn by the `plan-page` entry and the app's
// DEV-ONLY capture. Pictures = the committed samples (EXP-1654,
// specimen-samples.ts), never fetched.

const message: TreeMessage = {
  id: `plan-specimen`,
  sessionId: `plan-specimen-session`,
  authorKind: `agent`,
  authorId: null,
  body: ``,
  card: PLAN_PAGE_SPECIMEN.card as unknown as Record<string, unknown>,
  answeredAt: null,
  consumedAt: null,
  createdAt: new Date(0).toISOString(),
  authorSessionId: `plan-specimen-session`,
}
const row = threadAskRow(message) as Extract<ReturnType<typeof threadAskRow>, { type: `plan` }>

const noop = () => {}

const text = (markdown: string) => (
  <div className="flex flex-col gap-2">
    {markdown.split(/\n{2,}/).map((block, at) =>
      block.startsWith(`# `) ? (
        <h1 key={at} className="text-xl font-semibold">
          {block.slice(2)}
        </h1>
      ) : block.split(`\n`).every((line) => line.startsWith(`- `)) ? (
        // #EXP-1553: a list block draws real bullets like the app's markdown
        // (the natives' specimens), never literal hyphens.
        <ul key={at} className="list-disc pl-5">
          {block.split(`\n`).map((line, item) => (
            <li key={item}>{line.slice(2)}</li>
          ))}
        </ul>
      ) : (
        <p key={at} className="whitespace-pre-wrap">
          {block}
        </p>
      )
    )}
  </div>
)

export function PlanPageSpecimen({ size = `wide` }: { size?: `wide` | `phone` }) {
  const wide = size === `wide`
  // Every attachment url → its sample picture.
  const attachmentSrc = useSpecimenAttachmentSrc()
  const plan = useMemo(() => ({ ...row, plan: row.plan.replace(/\/api\/attachments\/([\w-]+)/g, (_, id: string) => attachmentSrc(id)) }), [attachmentSrc])
  return (
    <div
      data-testid="plan-page-specimen"
      className="relative flex flex-col overflow-hidden rounded-3xl border border-glass-stroke-card bg-glass-card-opaque"
      style={{ width: wide ? 1280 : 390, height: wide ? 720 : 760, maxWidth: `100%` }}
    >
      <div className={cn(`shrink-0 border-b border-border`, wide ? `px-10 pt-6 pb-3` : `px-3 pt-4 pb-2`)}>
        <div className="text-lg font-semibold">{PLAN_PAGE_SPECIMEN.chatName}</div>
        <ThreadTabs
          className="mt-4"
          variant={wide ? `bar` : `pager`}
          conversation={{ label: THREAD_COPY.conversation, active: false, onSelect: noop }}
          plan={{ label: planRowLabel(plan.plan), active: true, waiting: true, onSelect: noop }}
          issues={[]}
          stories={{ label: STORY_TAB.copy.label, active: false, dot: false, onSelect: noop }}
        />
      </div>
      <PlanPage
        className="min-h-0 flex-1"
        row={plan}
        mobile={!wide}
        renderText={text}
        attachmentSrc={attachmentSrc}
        busy={false}
        onDecide={() => {}}
        initialActive={PLAN_PAGE_SPECIMEN.activeSection}
      />
    </div>
  )
}
