import type { IssueEvent, Label, Board, User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"
import { conceptIcon } from "@/lib/icons.generated"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"

// EXP-317: timeline glyphs come from the shared registry, so a status change
// (or a board move) looks the same here as it does in the desktop IDE.
const StatusChangedIcon = conceptIcon(`event-status-changed`)
const AssigneeChangedIcon = conceptIcon(`event-assignee-changed`)
const LabelIcon = conceptIcon(`event-label-added`)
const BoardMovedIcon = conceptIcon(`event-board-moved`)
const PrOpenedIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PriorityChangedIcon = conceptIcon(`event-priority-changed`)

// Priority wire values render capitalized ("urgent" → "Urgent"); anything
// unexpected falls back to the raw string.
function priorityLabel(value: unknown): string {
  const raw = String(value ?? ``)
  return raw ? raw.charAt(0).toUpperCase() + raw.slice(1) : `None`
}

// EXP-314: `status_changed` payloads now carry the human status NAMES
// (`fromName`/`toName`) alongside the legacy enum anchors, so a custom status
// reads as itself. Rows written before EXP-314 have no names — fall back to
// the enum munge.
function statusLabel(payload: Record<string, unknown>, side: `to` | `from`): string {
  const name = payload[side === `to` ? `toName` : `fromName`]
  if (typeof name === `string` && name.length > 0) return name
  return String(payload[side] ?? ``).replace(/_/g, ` `)
}

function optionalString(value: unknown): string | null {
  return typeof value === `string` && value.length > 0 ? value : null
}

// EXP-525: the PR events carry their pull request, so the phrase links out to
// GitHub. `pr_opened` writes {prUrl, prNumber, branch}, `pr_merged` only
// {prUrl} — and rows from before either field existed carry neither, so both
// sides degrade to plain text. The number is parsed off the url when the
// payload has none.
function prRef(payload: Record<string, unknown>): {
  url: string | null
  number: number | null
} {
  const url = optionalString(payload.prUrl) ?? optionalString(payload.url)
  const raw = payload.prNumber ?? payload.number
  const parsed =
    typeof raw === `number`
      ? raw
      : typeof raw === `string`
        ? Number.parseInt(raw, 10)
        : NaN
  if (Number.isFinite(parsed) && parsed > 0) return { url, number: parsed }
  const fromUrl = url?.match(/\/pull\/(\d+)/)
  return { url, number: fromUrl ? Number(fromUrl[1]) : null }
}

function PrPhrase({
  verb,
  payload,
}: {
  verb: `opened` | `merged`
  payload: Record<string, unknown>
}) {
  const { url, number } = prRef(payload)
  const phrase = number ? `pull request #${number}` : `pull request`
  return (
    <>
      {verb}{` `}
      {url ? (
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          className="font-medium text-brand-soft hover:underline"
        >
          {phrase}
        </a>
      ) : (
        <span className="font-medium text-foreground">{phrase}</span>
      )}
    </>
  )
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
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const actor = event.actorUserId ? userMap.get(event.actorUserId) : undefined
  const actorName = displayUserName(actor, event.actorUserId)
  const payload = (event.payload ?? {}) as Record<string, unknown>

  let Icon = StatusChangedIcon
  // A resolved status row wins over the generic glyph, so the timeline shows
  // the same colored icon the list and the picker do (EXP-525). The shared
  // fallback chain (statusId row → anchor enum row → constructed default)
  // never fails, so the generic icon only survives for the other event types.
  let icon: React.ReactNode = null
  let text: React.ReactNode = null

  switch (event.type) {
    case `status_changed`: {
      const option = resolveStatus({
        status: String(payload.to ?? ``),
        statusId: optionalString(payload.toStatusId),
      })
      icon = (
        <StatusIcon option={option} className="!h-3.5 !w-3.5 shrink-0" />
      )
      text = (
        <>
          changed status to{` `}
          <span className="font-medium text-foreground">
            {statusLabel(payload, `to`)}
          </span>
        </>
      )
      break
    }
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
      text = <PrPhrase verb="opened" payload={payload} />
      break
    case `pr_merged`:
      Icon = PrMergedIcon
      text = <PrPhrase verb="merged" payload={payload} />
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
    case `priority_changed`: {
      Icon = PriorityChangedIcon
      text = (
        <>
          changed priority from{` `}
          <span className="font-medium text-foreground">
            {priorityLabel(payload.from)}
          </span>
          {` `}to{` `}
          <span className="font-medium text-foreground">
            {priorityLabel(payload.to)}
          </span>
        </>
      )
      break
    }
    // EXP-530: `created` rows are the automation-trigger substrate — the
    // issue header already shows creation, so the timeline suppresses them
    // on every client.
    case `created`:
      return null
    default:
      return null
  }

  return (
    <div className="flex items-center gap-2 py-1 pl-1 text-xs text-muted-foreground">
      {icon ?? <Icon className="size-3.5 shrink-0" />}
      <span className="truncate">
        <span className="font-medium text-foreground">{actorName}</span> {text}
      </span>
    </div>
  )
}
