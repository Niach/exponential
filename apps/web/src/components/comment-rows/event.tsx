import type { IssueEvent, Label, Board, User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"
import { conceptIcon } from "@/lib/icons.generated"

// EXP-317: timeline glyphs come from the shared registry, so a status change
// (or a board move) looks the same here as it does in the desktop IDE.
const StatusChangedIcon = conceptIcon(`event-status-changed`)
const AssigneeChangedIcon = conceptIcon(`event-assignee-changed`)
const LabelIcon = conceptIcon(`event-label-added`)
const BoardMovedIcon = conceptIcon(`event-board-moved`)
const PrOpenedIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)

// EXP-314: `status_changed` payloads now carry the human status NAMES
// (`fromName`/`toName`) alongside the legacy enum anchors, so a custom status
// reads as itself. Rows written before EXP-314 have no names — fall back to
// the enum munge.
function statusLabel(payload: Record<string, unknown>, side: `to` | `from`): string {
  const name = payload[side === `to` ? `toName` : `fromName`]
  if (typeof name === `string` && name.length > 0) return name
  return String(payload[side] ?? ``).replace(/_/g, ` `)
}

// A compact, single-line activity entry (status/assignee/label/PR).
export function EventRow({
  event,
  userMap,
  labelMap,
  boardMap,
}: {
  event: IssueEvent
  userMap: Map<string, User>
  labelMap: Map<string, Label>
  boardMap?: Map<string, Board>
}) {
  const actor = event.actorUserId ? userMap.get(event.actorUserId) : undefined
  const actorName = displayUserName(actor, event.actorUserId)
  const payload = (event.payload ?? {}) as Record<string, unknown>

  let Icon = StatusChangedIcon
  let text: React.ReactNode = null

  switch (event.type) {
    case `status_changed`:
      Icon = StatusChangedIcon
      text = (
        <>
          changed status to{` `}
          <span className="font-medium text-foreground">
            {statusLabel(payload, `to`)}
          </span>
        </>
      )
      break
    case `assignee_changed`: {
      Icon = AssigneeChangedIcon
      // `payload.to` can reference a user the viewer can't see (the users
      // shape only exposes co-members) — that's still an assignment, not a
      // removal.
      const to = payload.to ? userMap.get(String(payload.to)) : undefined
      text = payload.to ? (
        <>
          assigned{` `}
          <span className="font-medium text-foreground">
            {displayUserName(to, String(payload.to))}
          </span>
        </>
      ) : (
        <>removed the assignee</>
      )
      break
    }
    case `label_added`:
    case `label_removed`: {
      Icon = LabelIcon
      const label = payload.labelId
        ? labelMap.get(String(payload.labelId))
        : undefined
      text = (
        <>
          {event.type === `label_added` ? `added` : `removed`} label{` `}
          <span className="font-medium text-foreground">
            {label?.name ?? `a label`}
          </span>
        </>
      )
      break
    }
    case `pr_opened`:
      Icon = PrOpenedIcon
      text = <>opened a pull request</>
      break
    case `pr_merged`:
      Icon = PrMergedIcon
      text = <>merged the pull request</>
      break
    case `board_moved`: {
      Icon = BoardMovedIcon
      // A deleted source board leaves no name behind — fall back
      // generically (the payload's from/toIdentifier keeps the row useful).
      const fromBoard = payload.fromBoardId
        ? boardMap?.get(String(payload.fromBoardId))
        : undefined
      const toBoard = payload.toBoardId
        ? boardMap?.get(String(payload.toBoardId))
        : undefined
      const fromIdentifier = payload.fromIdentifier
        ? String(payload.fromIdentifier)
        : null
      text = (
        <>
          moved this from{` `}
          <span className="font-medium text-foreground">
            {fromBoard?.name ?? `another board`}
          </span>
          {fromIdentifier ? ` (${fromIdentifier})` : ``} to{` `}
          <span className="font-medium text-foreground">
            {toBoard?.name ?? `this board`}
          </span>
        </>
      )
      break
    }
    default:
      return null
  }

  return (
    <div className="flex items-center gap-2 py-1 pl-1 text-xs text-muted-foreground">
      <Icon className="size-3.5 shrink-0" />
      <span className="truncate">
        <span className="font-medium text-foreground">{actorName}</span> {text}
      </span>
    </div>
  )
}
