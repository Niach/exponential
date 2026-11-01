import { PLAN_PAGE_SPECIMEN } from "@exp/domain-contract/plan-page"
import { threadAskRow, type TreeMessage } from "@exp/domain-contract/session-tree"
import { PlanPage } from "./plan-page"

// #EXP-1493: THE plan page's styleguide specimen (plan-page.json
// `specimen`), wide and phone, drawn by the `plan-page` entry and the app's
// DEV-ONLY capture. Pictures = placeholders (a transparent pixel over the
// tiles' muted fill).

const PLACEHOLDER = `data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7`

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
// The specimen's pictures are placeholders: every attachment url → the fill.
const plan = { ...row, plan: row.plan.replace(/\/api\/attachments\/[\w-]+/g, PLACEHOLDER) }

const text = (markdown: string) => (
  <div className="flex flex-col gap-2">
    {markdown.split(/\n{2,}/).map((block, at) =>
      block.startsWith(`# `) ? (
        <h1 key={at} className="text-xl font-semibold">
          {block.slice(2)}
        </h1>
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
  return (
    <div
      data-testid="plan-page-specimen"
      className="relative overflow-hidden rounded-3xl border border-glass-stroke-card bg-background"
      style={{ width: wide ? 1280 : 390, height: wide ? 720 : 760, maxWidth: `100%` }}
    >
      <PlanPage
        row={plan}
        chatName={PLAN_PAGE_SPECIMEN.chatName}
        mobile={!wide}
        renderText={text}
        busy={false}
        onDecide={() => {}}
        onBack={() => {}}
        onClose={wide ? () => {} : undefined}
        onOpenChat={() => {}}
        initialActive={PLAN_PAGE_SPECIMEN.activeSection}
      />
    </div>
  )
}
