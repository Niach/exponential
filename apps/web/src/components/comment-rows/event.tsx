import {
  AlertTriangle,
  CircleDot,
  CornerDownLeft,
  GitMerge,
  GitPullRequest,
  HelpCircle,
  Play,
  Sparkles,
  Tag,
  UserPlus,
} from "lucide-react"
import type { IssueEvent, Label, User } from "@/db/schema"

function statusLabel(s: string): string {
  return s.replace(/_/g, ` `)
}

// A compact, single-line activity entry (status/assignee/label/PR/plan/error).
export function EventRow({
  event,
  userMap,
  labelMap,
}: {
  event: IssueEvent
  userMap: Map<string, User>
  labelMap: Map<string, Label>
}) {
  const actor = event.actorUserId ? userMap.get(event.actorUserId) : undefined
  const actorName = actor?.name || actor?.email || `Someone`
  const payload = (event.payload ?? {}) as Record<string, unknown>

  let Icon = CircleDot
  let text: React.ReactNode = null

  switch (event.type) {
    case `status_changed`:
      Icon = CircleDot
      text = (
        <>
          changed status to{` `}
          <span className="font-medium text-foreground">
            {statusLabel(String(payload.to ?? ``))}
          </span>
        </>
      )
      break
    case `assignee_changed`: {
      Icon = UserPlus
      // `payload.to` can reference a user the viewer can't see (the users
      // shape only exposes co-members) — that's still an assignment, not a
      // removal.
      const to = payload.to ? userMap.get(String(payload.to)) : undefined
      text = payload.to ? (
        <>
          assigned{` `}
          <span className="font-medium text-foreground">
            {to ? to.name || to.email : `someone`}
          </span>
        </>
      ) : (
        <>removed the assignee</>
      )
      break
    }
    case `label_added`:
    case `label_removed`: {
      Icon = Tag
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
    case `plan_ready`:
      Icon = Sparkles
      text = <>posted a plan for review</>
      break
    case `agent_started`:
      Icon = Play
      text = <>started working</>
      break
    case `agent_question`:
      Icon = HelpCircle
      text = <>asked a question</>
      break
    case `agent_answer`:
      Icon = CornerDownLeft
      text = <>answered the agent</>
      break
    case `pr_opened`:
      Icon = GitPullRequest
      text = <>opened a pull request</>
      break
    case `pr_merged`:
      Icon = GitMerge
      text = <>merged the pull request</>
      break
    case `agent_error`:
      Icon = AlertTriangle
      text = <>hit an error</>
      break
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
