import { useState } from "react"
import {
  AgentRunMark,
  EmptyState,
  GlassSectionHeader,
  ListRow,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  cn,
  conceptIcon,
  treeGuides,
} from "@exp/ui"
import { prConcept, type RunDetail } from "./model"
import {
  SESSION_STATUS_TONE_CLASS,
  blockedBadgeLabel,
  pastRunByline,
  sessionAgentCaption,
  sessionBands,
  sessionDisplayState,
  sessionIdentity,
  sessionRowIsWorking,
  sessionStatusLine,
  type SessionListRow,
} from "./list-session"

const RunsIcon = conceptIcon(`coding-running`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

// EXP-1183 — `exponential_sessions_list` as the app's session list
// (`session-list-rows.tsx`): Running / In review / Ended group bands over flat
// rows; a live row = the agent's run mark (brand + state badge, the working
// spark mid-turn), identifier + subject, the agent caption and the toned
// status line; an ended row = identity + `<device> · <when>`. Child runs nest
// under their parent with the EXP-965 tree guides. Rows carry no buttons: a
// row opens the run (the host calls `exponential_sessions_get`).
export function RunsListView({
  runs,
  onOpen,
}: {
  runs: readonly RunDetail[]
  onOpen?: (run: RunDetail) => void
}) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const bands = sessionBands(runs as readonly SessionListRow[], collapsed)
  if (bands.length === 0) {
    return (
      <EmptyState
        icon={RunsIcon}
        title="No runs"
        description="Nothing has run on these teams yet."
      />
    )
  }
  const toggle = (id: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next
    })
  return (
    <div className="flex flex-col gap-3 p-2">
      {bands.map((band) => {
        const guides = treeGuides(band.rows.map((entry) => entry.depth))
        return (
          <section key={band.band} className="flex flex-col">
            <GlassSectionHeader label={band.label} count={band.count} />
            {band.rows.map((entry, index) => (
              <SessionRow
                key={entry.row.id}
                row={entry.row}
                depth={entry.depth}
                guide={guides[index]}
                expandable={entry.hasChildren}
                expanded={!collapsed.has(entry.row.id)}
                onToggle={() => toggle(entry.row.id)}
                onOpen={() => onOpen?.(entry.row)}
              />
            ))}
          </section>
        )
      })}
    </div>
  )
}

function SessionRow({
  row,
  depth,
  guide,
  expandable,
  expanded,
  onToggle,
  onOpen,
}: {
  row: SessionListRow
  depth: number
  guide: ReturnType<typeof treeGuides>[number] | undefined
  expandable: boolean
  expanded: boolean
  onToggle: () => void
  onOpen: () => void
}) {
  const identity = sessionIdentity(row)
  const live = row.status !== `ended`
  const state = sessionDisplayState(row)
  const caption = live ? sessionAgentCaption(row) : null
  const status = live ? sessionStatusLine(row) : null
  const blocked = live ? blockedBadgeLabel(row.blocked) : null
  const byline = live ? null : pastRunByline(row)
  const pr = prConcept(row.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  return (
    <ListRow
      interactive
      onClick={onOpen}
      className="relative gap-2 px-3 py-2.5"
      style={{ paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }}
      data-testid={`run-row-${row.id}`}
    >
      <TreeGuides guide={guide} />
      {expandable && (
        <span
          role="button"
          tabIndex={-1}
          aria-label={expanded ? `Collapse child runs` : `Expand child runs`}
          className="flex shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
          onClick={(event) => {
            event.stopPropagation()
            onToggle()
          }}
        >
          {expanded ? (
            <ChevronDownIcon className="size-3" />
          ) : (
            <ChevronRightIcon className="size-3" />
          )}
        </span>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className={cn(`flex shrink-0 items-center justify-center`, !live && `opacity-60`)}>
            {/* EXP-1184: the agent's mark wears the run's state badge; an
                ended run keeps the bare (dimmed) brand mark. */}
            <AgentRunMark
              agent={row.agent}
              state={live ? (sessionRowIsWorking(row) ? `working` : state) : undefined}
              ringClassName="ring-background"
            />
          </span>
          {identity.identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identity.identifier}
            </span>
          )}
          <span className={cn(`truncate`, live && `font-medium`)}>{identity.subject}</span>
          {PrIcon && row.prNumber != null && (
            <span className="ml-auto flex shrink-0 items-center gap-1 pl-2 text-xs text-muted-foreground">
              <PrIcon className="size-3.5" />#{row.prNumber}
            </span>
          )}
        </div>
        {caption && (
          <div className="truncate pl-5 text-xs text-muted-foreground" title={caption}>
            {caption}
          </div>
        )}
        {status && (
          <div className={cn(`truncate pl-5 text-xs`, SESSION_STATUS_TONE_CLASS[status.tone])}>
            {status.text}
          </div>
        )}
        {blocked && (
          <div className="truncate pl-5 text-xs font-medium text-amber-400">{blocked}</div>
        )}
        {byline && (
          <div className="truncate pl-5 text-xs text-muted-foreground">{byline}</div>
        )}
      </div>
    </ListRow>
  )
}
