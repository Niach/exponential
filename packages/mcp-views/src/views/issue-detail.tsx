// EXP-1153: one issue, in full — the issue view's body and the board view's
// detail panel. Real @exp/ui parts (StatusGlyph, UserAvatar, Badge, Button);
// the description stays plain text here (the Markdown round-trip set is a
// web-app concern; a host shows Markdown in the chat already).

import type { ReactNode } from "react"
import type { IconName } from "@exp/icons"
import { Button } from "@exp/ui/src/button"
import { StatusGlyph } from "@exp/ui/src/status-glyph"
import { UserAvatar } from "@exp/ui/src/user-avatar"
import { cn } from "@exp/ui/src/cn"
import type { Bridge } from "../bridge"
import type { IssueViewData, ViewIssue } from "../contract"

/** A small outlined tag; @exp/ui's `Badge` is the COUNT capsule, not this. */
export function Tag({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <span
      className={cn(
        `inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border border-border px-1.5 text-[10px] leading-none text-muted-foreground`,
        className
      )}
    >
      {children}
    </span>
  )
}

export function PriorityBadge({ priority }: { priority: string }) {
  if (!priority || priority === `none`) return null
  return (
    <Tag
      className={cn(
        `font-mono uppercase tracking-wide`,
        priority === `urgent` && `border-destructive/60 text-destructive`,
        priority === `high` && `border-amber-500/60 text-amber-500`
      )}
    >
      {priority}
    </Tag>
  )
}

export function LabelDots({ labels }: { labels: ViewIssue[`labels`] }) {
  return (
    <>
      {labels.map((label) => (
        <span key={label.id} className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          <span className="size-2 rounded-full" style={{ background: label.color }} />
          {label.name}
        </span>
      ))}
    </>
  )
}

export function PrBadge({ pr }: { pr: ViewIssue[`pr`] }) {
  if (!pr) return null
  return (
    <Tag className="border-transparent bg-muted">
      PR {pr.state}
      {pr.number ? ` #${pr.number}` : ``}
    </Tag>
  )
}

export function IssueDetail({
  data,
  bridge,
  onClose,
  compact,
}: {
  data: IssueViewData
  bridge: Bridge
  onClose?: () => void
  /** Inside the board panel: smaller title, fewer comments. */
  compact?: boolean
}) {
  const { issue, status, comments, relations } = data
  const shownComments = compact ? comments.slice(0, 3) : comments
  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">{issue.identifier}</span>
          <span className="inline-flex items-center gap-1">
            <StatusGlyph icon={status.icon as IconName} colorHex={status.color} className="size-3.5" />
            {status.name}
          </span>
          <PriorityBadge priority={issue.priority} />
          <PrBadge pr={issue.pr} />
          {issue.dueDate && <span>due {issue.dueDate}</span>}
        </div>
        <h2 className={cn(`font-semibold leading-snug`, compact ? `text-sm` : `text-base`)}>{issue.title}</h2>
        <div className="flex flex-wrap items-center gap-2">
          {issue.assignee && (
            <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
              <UserAvatar user={issue.assignee} size={20} />
              {issue.assignee.name ?? issue.assignee.email}
            </span>
          )}
          <LabelDots labels={issue.labels} />
        </div>
      </div>

      <p className="whitespace-pre-wrap text-sm text-muted-foreground">
        {issue.description?.trim() ? issue.description : `No description.`}
      </p>

      {relations.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {relations.map((r) => (
            <Tag key={`${r.type}-${r.direction}-${r.otherIdentifier}`} className="text-[11px]">
              <span>{relationLabel(r.type, r.direction)}</span>
              <span className="font-mono text-foreground">{r.otherIdentifier}</span>
            </Tag>
          ))}
        </div>
      )}

      {shownComments.length > 0 && (
        <div className="flex flex-col divide-y divide-border rounded-md border border-border">
          {shownComments.map((c) => (
            <div key={c.id} className="flex gap-2 p-2">
              <UserAvatar user={c.author} size={20} className="mt-0.5" />
              <div className="min-w-0 flex-1">
                <div className="text-xs text-muted-foreground">
                  {c.author?.name ?? c.author?.email ?? `Someone`} · {relativeTime(c.createdAt)}
                  {c.source === `mcp` ? ` · via MCP` : ``}
                </div>
                <div className="whitespace-pre-wrap text-sm">{c.body}</div>
              </div>
            </div>
          ))}
          {compact && comments.length > shownComments.length && (
            <div className="p-2 text-xs text-muted-foreground">
              {comments.length - shownComments.length} more in Exponential
            </div>
          )}
        </div>
      )}

      <div className="flex flex-wrap gap-1.5">
        <Button size="sm" variant="secondary" onClick={() => void bridge.openLink(issue.url)}>
          Open in Exponential
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void bridge.sendMessage(`Summarize ${issue.identifier} and its comments.`)}
        >
          Summarize in chat
        </Button>
        {onClose && (
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        )}
      </div>
    </div>
  )
}

function relationLabel(type: string, direction: string): string {
  const inverse = direction === `inverse` || direction === `incoming`
  switch (type) {
    case `blocks`:
      return inverse ? `blocked by` : `blocks`
    case `parent`:
      return inverse ? `sub-issue` : `parent`
    case `duplicate`:
      return inverse ? `duplicated by` : `duplicate of`
    default:
      return `related`
  }
}

export function relativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime()
  if (!Number.isFinite(ms)) return ``
  const min = Math.round(ms / 60000)
  if (min < 1) return `just now`
  if (min < 60) return `${min}m ago`
  const h = Math.round(min / 60)
  if (h < 24) return `${h}h ago`
  const d = Math.round(h / 24)
  if (d < 30) return `${d}d ago`
  return new Date(iso).toLocaleDateString()
}
