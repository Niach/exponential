import {
  CircleDot,
  FolderInput,
  GitMerge,
  GitPullRequest,
  Rocket,
  Tag,
  UserPlus,
} from "lucide-react"
import type { IssueEvent, Label, Project, Release, User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"

function statusLabel(s: string): string {
  return s.replace(/_/g, ` `)
}

// A compact, single-line activity entry (status/assignee/label/PR).
export function EventRow({
  event,
  userMap,
  labelMap,
  releaseMap,
  projectMap,
}: {
  event: IssueEvent
  userMap: Map<string, User>
  labelMap: Map<string, Label>
  releaseMap?: Map<string, Release>
  projectMap?: Map<string, Project>
}) {
  const actor = event.actorUserId ? userMap.get(event.actorUserId) : undefined
  const actorName = displayUserName(actor, event.actorUserId)
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
    case `pr_opened`:
      Icon = GitPullRequest
      text = <>opened a pull request</>
      break
    case `pr_merged`:
      Icon = GitMerge
      text = <>merged the pull request</>
      break
    case `release_added`:
    case `release_removed`: {
      Icon = Rocket
      // A deleted release leaves no name behind — fall back generically.
      const release = payload.releaseId
        ? releaseMap?.get(String(payload.releaseId))
        : undefined
      text = (
        <>
          {event.type === `release_added`
            ? `added this to release`
            : `removed this from release`}{` `}
          <span className="font-medium text-foreground">
            {release?.name ?? `a release`}
          </span>
        </>
      )
      break
    }
    case `project_moved`: {
      Icon = FolderInput
      // A deleted source project leaves no name behind — fall back
      // generically (the payload's from/toIdentifier keeps the row useful).
      const fromProject = payload.fromProjectId
        ? projectMap?.get(String(payload.fromProjectId))
        : undefined
      const toProject = payload.toProjectId
        ? projectMap?.get(String(payload.toProjectId))
        : undefined
      const fromIdentifier = payload.fromIdentifier
        ? String(payload.fromIdentifier)
        : null
      text = (
        <>
          moved this from{` `}
          <span className="font-medium text-foreground">
            {fromProject?.name ?? `another project`}
          </span>
          {fromIdentifier ? ` (${fromIdentifier})` : ``} to{` `}
          <span className="font-medium text-foreground">
            {toProject?.name ?? `this project`}
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
