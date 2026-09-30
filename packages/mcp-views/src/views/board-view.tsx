// EXP-1153: the board view — open issues grouped by status ROW, the way the
// web board and the natives group them (lists group by status row, not by
// category), drawn with the same IssueGroupBand the web app uses.

import { useState } from "react"
import type { IconName } from "@exp/icons"
import { Button } from "@exp/ui/src/button"
import { IssueGroupBand } from "@exp/ui/src/issue-group-band"
import { UserAvatar } from "@exp/ui/src/user-avatar"
import { cn } from "@exp/ui/src/cn"
import { hexWithAlpha } from "@exp/ui/src/status-icons"
import type { ViewProps } from "../shell"
import { Empty } from "../shell"
import { structuredOf } from "../bridge"
import { VIEW_TOOLS, type BoardViewData, type IssueViewData, type ViewIssue } from "../contract"
import { IssueDetail, LabelDots, PrBadge, PriorityBadge } from "./issue-detail"

const CATEGORY_LABEL: Record<string, string> = {
  backlog: `Backlog`,
  unstarted: `Todo`,
  started: `Started`,
  completed: `Done`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

export function BoardView({ data, refresh, bridge }: ViewProps<BoardViewData>) {
  const [filter, setFilter] = useState<string | null>(null)
  const [closed, setClosed] = useState<Set<string>>(new Set())
  const [selected, setSelected] = useState<string | null>(null)
  const [detail, setDetail] = useState<IssueViewData | null>(null)
  const [detailError, setDetailError] = useState<string | null>(null)

  const byStatus = new Map<string, ViewIssue[]>()
  for (const issue of data.issues) {
    if (filter) {
      const status = data.statuses.find((s) => s.id === issue.statusId)
      if (status?.category !== filter) continue
    }
    const list = byStatus.get(issue.statusId) ?? []
    list.push(issue)
    byStatus.set(issue.statusId, list)
  }
  const groups = data.statuses
    .filter((status) => byStatus.has(status.id))
    .map((status) => ({ status, issues: byStatus.get(status.id)! }))
  const categories = [...new Set(data.statuses.filter((s) => byStatus.size === 0 || data.issues.some((i) => i.statusId === s.id)).map((s) => s.category))]

  const select = async (issue: ViewIssue) => {
    if (selected === issue.identifier) {
      setSelected(null)
      setDetail(null)
      return
    }
    setSelected(issue.identifier)
    setDetail(null)
    setDetailError(null)
    try {
      const result = await bridge.callTool<IssueViewData>(VIEW_TOOLS.issue, { id: issue.identifier })
      const loaded = structuredOf<IssueViewData>(result)
      if (result.isError || !loaded) {
        setDetailError(result.content?.[0]?.text ?? `Could not load ${issue.identifier}.`)
      } else {
        setDetail(loaded)
        void bridge.updateModelContext(
          `The user opened ${issue.identifier} ("${issue.title}") in the Exponential board view.`
        ).catch(() => {})
      }
    } catch (e) {
      setDetailError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className="flex flex-col">
      <header className="flex items-center gap-2 border-b border-border px-3 py-2">
        {/* Not BoardGlyph: its icon table imports the zod-backed domain (+266 KB). */}
        <span
          className="grid size-5 shrink-0 place-items-center rounded-md text-[10px] font-semibold text-white"
          style={{ background: data.board.color }}
        >
          {data.board.prefix.slice(0, 2)}
        </span>
        <h1 className="text-sm font-semibold">{data.board.name}</h1>
        <span className="text-xs text-muted-foreground">{data.issues.length} open</span>
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={() => void refresh()}>
          Refresh
        </Button>
        <Button size="sm" variant="outline" onClick={() => void bridge.openLink(data.board.url)}>
          Open in Exponential
        </Button>
      </header>

      <div className="flex flex-wrap gap-1.5 border-b border-border px-3 py-2">
        <FilterChip active={filter === null} onClick={() => setFilter(null)}>
          All
        </FilterChip>
        {categories.map((category) => (
          <FilterChip key={category} active={filter === category} onClick={() => setFilter(category)}>
            {CATEGORY_LABEL[category] ?? category}
          </FilterChip>
        ))}
      </div>

      {groups.length === 0 && <Empty>Nothing open here.</Empty>}

      {groups.map(({ status, issues }) => {
        const open = !closed.has(status.id)
        return (
          <section key={status.id} className="border-b border-border last:border-b-0">
            <IssueGroupBand
              glyph={{ icon: status.icon as IconName, colorHex: status.color }}
              name={status.name}
              count={issues.length}
              open={open}
              onToggle={() =>
                setClosed((prev) => {
                  const next = new Set(prev)
                  if (next.has(status.id)) next.delete(status.id)
                  else next.add(status.id)
                  return next
                })
              }
              wash={{ style: { background: hexWithAlpha(status.color, 0.1) } }}
            />
            {open &&
              issues.map((issue) => (
                <button
                  type="button"
                  key={issue.id}
                  aria-pressed={selected === issue.identifier}
                  onClick={() => void select(issue)}
                  className={cn(
                    `grid w-full grid-cols-[4.5rem_1fr_auto] items-center gap-2 border-t border-border px-3 py-1.5 text-left first:border-t-0 hover:bg-muted/60`,
                    selected === issue.identifier && `bg-muted/60`
                  )}
                >
                  <span className="font-mono text-xs text-muted-foreground">{issue.identifier}</span>
                  <span className="truncate text-sm">{issue.title}</span>
                  <span className="flex items-center gap-1.5">
                    <PrBadge pr={issue.pr} />
                    <LabelDots labels={issue.labels.slice(0, 2)} />
                    <PriorityBadge priority={issue.priority} />
                    {issue.assignee && <UserAvatar user={issue.assignee} size={20} />}
                  </span>
                </button>
              ))}
          </section>
        )
      })}

      {selected && (
        <div className="border-t border-border">
          {detailError && <div className="p-3 text-sm text-destructive">{detailError}</div>}
          {!detailError && !detail && <Empty>Loading {selected}…</Empty>}
          {detail && (
            <IssueDetail
              data={detail}
              bridge={bridge}
              compact
              onClose={() => {
                setSelected(null)
                setDetail(null)
              }}
            />
          )}
        </div>
      )}
    </div>
  )
}

function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: string
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        `rounded-full border border-border px-2.5 py-0.5 text-xs text-muted-foreground hover:bg-muted`,
        active && `border-muted-foreground bg-muted text-foreground`
      )}
    >
      {children}
    </button>
  )
}
