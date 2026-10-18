import { useMemo, type ReactNode } from "react"
import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { PLAN_CARD_COPY, PLAN_CARD_GEOMETRY } from "@exp/domain-contract/plan-card"
import { buildSessionThread } from "@exp/domain-contract/session-tree"
import { ThreadConversation } from "./session-thread-view"

// EXP-1280: THE rich plan / question card specimen (fixtures/plan-card.json
// ×4): session-thread.json's case "EXP-1280: a rich plan card…" through the
// REAL row model and `ThreadConversation`: the pending plan (a picture in
// the body, the inline surface, a picture-tile question + a multiple one),
// the answered multi-pick question (closed) and the approved plan with its
// feedback. The styleguide entry and the app's `?dev=plan-card` capture
// render it; the natives' specimens draw the same rows.

const RICH_CASE = `EXP-1280: a rich plan card`

/** Stand-in pictures = THE palette ×4 (iOS/Android/desktop specimens draw the
 *  same): one gradient per attachment id, a 35% white bar, a 12% white panel. */
const TONES: Record<string, [string, string]> = {
  "att-1": [`#3f3f46`, `#52525b`],
  "att-2": [`#1e3a5f`, `#3b82f6`],
  "att-3": [`#14532d`, `#22c55e`],
}

export function planCardStandIn(attachmentId: string): string {
  const [from, to] = TONES[attachmentId] ?? [`#3f3f46`, `#71717a`]
  return `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/></linearGradient></defs><rect width="800" height="500" fill="url(#g)"/><rect x="48" y="48" width="320" height="28" rx="6" fill="#ffffff" fill-opacity="0.35"/><rect x="48" y="100" width="704" height="352" rx="12" fill="#ffffff" fill-opacity="0.12"/></svg>`
  )}`
}

const ATTACHMENT = /^!\[([^\]]*)\]\(\/api\/attachments\/([^)\s]+)\)$/

/** A specimen-sized stand-in for the app's GFM renderer (`.tiptap-chat`):
 *  a heading = `card.headingScale` × the body at weight 600, an attachment
 *  embed paints its stand-in picture like the app's inline image (full
 *  content width at its aspect, `card.pictureRadius` + `card.pictureStroke`,
 *  no max height). */
const text = (body: string): ReactNode => (
  <div className="flex flex-col gap-1">
    {body.split(`\n`).map((line, index) => {
      const trimmed = line.trim()
      if (trimmed === ``) return null
      const picture = ATTACHMENT.exec(trimmed)
      if (picture) {
        return (
          <img
            key={index}
            src={planCardStandIn(picture[2]!)}
            alt={picture[1]}
            width={800}
            height={500}
            className="my-1 block h-auto w-full border-solid border-glass-stroke-card"
            style={{
              borderRadius: PLAN_CARD_GEOMETRY.pictureRadius,
              borderWidth: PLAN_CARD_GEOMETRY.pictureStroke,
            }}
          />
        )
      }
      if (trimmed.startsWith(`#`)) {
        return (
          <p
            key={index}
            className="font-semibold"
            style={{ fontSize: `${PLAN_CARD_GEOMETRY.headingScale}em` }}
          >
            {trimmed.replace(/^#+\s*/, ``)}
          </p>
        )
      }
      return <p key={index}>{trimmed}</p>
    })}
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

export function PlanCardSpecimen() {
  const rows = useMemo(() => {
    const found = fixture.cases.find((c) => c.name.startsWith(RICH_CASE))
    return found ? buildSessionThread(found.input as never).rows : []
  }, [])
  const row = (id: string) => rows.filter((r) => r.id === id)
  return (
    <div data-testid="plan-card-specimen" className="flex max-w-2xl flex-col gap-6 text-sm">
      <Case label={PLAN_CARD_COPY.specimen.pending}>
        <ThreadConversation
          rows={row(`n1`)}
          renderText={text}
          attachmentSrc={planCardStandIn}
          onAnswer={noop}
          onDecide={noop}
        />
      </Case>
      <Case label={PLAN_CARD_COPY.specimen.answered}>
        <ThreadConversation rows={row(`n2`)} renderText={text} attachmentSrc={planCardStandIn} />
      </Case>
      <Case label={PLAN_CARD_COPY.specimen.approved}>
        <ThreadConversation rows={row(`n3`)} renderText={text} attachmentSrc={planCardStandIn} />
      </Case>
    </div>
  )
}
