import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import type * as React from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, CodingSession, Issue } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import {
  agentLabel,
  AgentBrandMark,
  conceptIcon,
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  LiveDot,
} from "@exp/ui"
import {
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import {
  closeTabs,
  groupedTabs,
  partitionUnits,
  routePathFromLocation,
  tabHref,
  tabIsLive,
  tabKey,
  type WorkTab,
} from "@/lib/work-tabs"
import {
  setTabGroupCollapsed,
  updateWorkTabs,
  useCollapsedTabGroups,
  useWorkTabs,
} from "@/hooks/use-work-tabs"
import {
  LIVE_DOT_TONE_BY_SESSION_TONE,
  RunningIndicator,
} from "@/components/agent-session-row"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"

// EXP-870: the WORK TABS strip — browser-like tabs on the bare ground above
// the content card, md+ only (the desktop's title-band strip; phones keep
// their tab bar). The model is `lib/work-tabs.ts`, the writer
// `work-tabs-sync.tsx`; this only draws it:
//
//   * EXP-877: the LIVE runs come first, GROUPED BY AGENT (`groupedTabs`) —
//     each group is its brand mark, its chips, then a `<` that folds the group
//     down to the mark alone (per team, per window). Navigating to a chip in a
//     folded group unfolds it, so the active chip is never hidden.
//   * the ordinary tabs follow in their stored order, the ACTIVE one derived
//     from the URL — never stored;
//   * a chip's lead is the live run's state dot (the ping while the agent
//     works), else the issue's status glyph, else a muted dot;
//   * a LIVE chip cannot be closed at all (EXP-877): no ×, no Close, and
//     Close others / Close all pass over it;
//   * chips that do not fit collapse into a trailing "+N" menu
//     (`partitionUnits`, measured against the real chips; the group marks
//     and chevrons are always drawn, and a folded chip measures zero and
//     never reaches that menu).

const UiCloseIcon = conceptIcon(`ui-close`)
const NavSupportIcon = conceptIcon(`nav-support`)
const UiChevronLeftIcon = conceptIcon(`ui-chevron-left`)

interface ChipModel {
  key: string
  tab: WorkTab
  identifier: string | null
  title: string
  lead: React.ReactNode
  /** EXP-877: a live chip is permanent — no ×, no Close. */
  live: boolean
  /** Inside a folded group: mounted at zero width, out of reach. */
  folded: boolean
}

/** One thing the strip lays out, in order. Group marks and chevrons are
 *  measured exactly like chips so the overflow packing keeps working. */
type StripUnit =
  | { kind: `group`; agent: string; collapsed: boolean }
  | { kind: `chip`; chip: ChipModel }
  | { kind: `chevron`; agent: string }

/** Support thread subjects, fetched once per thread per page load — the
 *  helpdesk tables are server-only (never synced). */
const threadSubjects = new Map<string, Promise<string | null>>()

function useThreadSubject(threadId: string | null): string | null {
  const [subject, setSubject] = useState<string | null>(null)
  useEffect(() => {
    if (!threadId) return
    let cancelled = false
    let pending = threadSubjects.get(threadId)
    if (!pending) {
      pending = trpc.helpdesk.getThread
        .query({ threadId }, { context: { skipErrorToast: true } })
        .then((detail) => detail.thread.title ?? null)
        .catch(() => null)
      threadSubjects.set(threadId, pending)
    }
    void pending.then((value) => {
      if (!cancelled) setSubject(value)
    })
    return () => {
      cancelled = true
    }
  }, [threadId])
  return subject
}

function SupportTitle({ threadId }: { threadId: string }) {
  const subject = useThreadSubject(threadId)
  return <>{subject?.trim() || `Support conversation`}</>
}

export function WorkTabsStrip({
  teamId,
  teamSlug,
  boards,
}: {
  teamId: string
  teamSlug: string
  boards: Board[] | undefined
}) {
  const navigate = useNavigate()
  const state = useWorkTabs(teamId)
  const collapsedGroups = useCollapsedTabGroups(teamId)

  const issueIds = useMemo(
    () =>
      [
        ...new Set(
          state.tabs.flatMap((tab) => (tab.kind === `issue` ? [tab.issueId] : []))
        ),
      ].sort(),
    [state.tabs]
  )
  const runIds = useMemo(
    () =>
      [
        ...new Set(
          state.tabs.flatMap((tab) =>
            tab.kind !== `support` && tab.runId ? [tab.runId] : []
          )
        ),
      ].sort(),
    [state.tabs]
  )
  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )
  const { data: runRows } = useLiveQuery(
    (query) =>
      runIds.length > 0
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => inArray(s.id, runIds))
        : undefined,
    [runIds.join(`,`)]
  )
  const issuesById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((row) => [row.id, row])),
    [issueRows]
  )
  const runsById = useMemo(
    () =>
      new Map(((runRows ?? []) as CodingSession[]).map((row) => [row.id, row])),
    [runRows]
  )
  const boardSlugById = useMemo(
    () => new Map((boards ?? []).map((board) => [board.id, board.slug])),
    [boards]
  )

  // EXP-877: the live tabs by agent (contract order), then everything else.
  const { groups, rest } = useMemo(
    () =>
      groupedTabs(state.tabs, (tab) =>
        tab.kind !== `support` && tab.runId
          ? (runsById.get(tab.runId)?.agent ?? null)
          : null
      ),
    [state.tabs, runsById]
  )
  const tabs = useMemo(
    () => [...groups.flatMap((group) => group.tabs), ...rest],
    [groups, rest]
  )

  // The active tab, off the URL alone.
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const activeKey = useMemo(() => {
    const path = routePathFromLocation(pathname, null)
    if (!path) return null
    const match = tabs.find((tab) => {
      switch (path.kind) {
        case `support`:
          return tab.kind === `support` && tab.threadId === path.threadId
        case `run`:
          return tab.kind !== `support` && tab.runId === path.runId
        case `issue`: {
          if (tab.kind !== `issue`) return false
          const issue = issuesById.get(tab.issueId)
          return (
            issue?.identifier === path.identifier &&
            boardSlugById.get(issue.boardId) === path.boardSlug
          )
        }
      }
    })
    return match ? tabKey(match) : null
  }, [pathname, tabs, issuesById, boardSlugById])

  // A live tab that becomes ACTIVE unfolds its group — and only then, so a
  // group folded by hand while reading one of its runs stays folded.
  const activeGroupAgent = useMemo(() => {
    const group = groups.find((entry) =>
      entry.tabs.some((tab) => tabKey(tab) === activeKey)
    )
    return group?.agent ?? null
  }, [activeKey, groups])
  // `activeKey` is a dependency in its own right: moving between two runs of
  // the SAME agent must unfold that group again, and on a cold load the runs
  // have not synced their agent yet — every live tab reads as claude until
  // they do, so the effect has to fire again when the real one resolves.
  useEffect(() => {
    if (activeGroupAgent) setTabGroupCollapsed(teamId, activeGroupAgent, false)
  }, [teamId, activeKey, activeGroupAgent])

  const collapsedSet = useMemo(
    () => new Set(collapsedGroups),
    [collapsedGroups]
  )
  const chipOf = (tab: WorkTab, folded: boolean): ChipModel => {
    const key = tabKey(tab)
    if (tab.kind === `support`) {
      return {
        key,
        tab,
        identifier: null,
        title: ``,
        lead: <NavSupportIcon className="size-3.5 text-muted-foreground" />,
        live: false,
        folded,
      }
    }
    const run = tab.runId ? runsById.get(tab.runId) : undefined
    const issue = tab.kind === `issue` ? issuesById.get(tab.issueId) : undefined
    const identity = run
      ? sessionIdentity({ session: run, issue })
      : issue
        ? { identifier: issue.identifier, subject: issue.title }
        : { identifier: null, subject: `Loading…` }
    return {
      key,
      tab,
      identifier: identity.identifier,
      title: identity.subject,
      lead: <ChipLead tab={tab} run={run} issue={issue} />,
      live: tabIsLive(tab),
      folded,
    }
  }

  const units: StripUnit[] = []
  for (const group of groups) {
    const collapsed = collapsedSet.has(group.agent)
    units.push({ kind: `group`, agent: group.agent, collapsed })
    for (const tab of group.tabs) {
      units.push({ kind: `chip`, chip: chipOf(tab, collapsed) })
    }
    if (!collapsed) units.push({ kind: `chevron`, agent: group.agent })
  }
  for (const tab of rest) units.push({ kind: `chip`, chip: chipOf(tab, false) })
  const chips = units.flatMap((unit) => (unit.kind === `chip` ? [unit.chip] : []))

  // ── Overflow: measure every unit in a hidden row, pack the visible ones.
  const containerRef = useRef<HTMLDivElement>(null)
  const measureRef = useRef<HTMLDivElement>(null)
  const overflowMeasureRef = useRef<HTMLButtonElement>(null)
  const [available, setAvailable] = useState(0)
  const [widths, setWidths] = useState<number[]>([])
  const [overflowW, setOverflowW] = useState(40)
  // `gap-1` is rem-based and the md+ root font is 1.15625rem, so the gap is
  // measured too rather than assumed.
  const [gap, setGap] = useState(4)
  const chipSignature = units
    .map((unit) =>
      unit.kind === `chip`
        ? `${unit.chip.key}|${unit.chip.title}|${unit.chip.folded ? `-` : `+`}`
        : `${unit.kind}:${unit.agent}`
    )
    .join(`\n`)

  const hasChips = units.length > 0
  const measureChips = () => {
    const node = measureRef.current
    if (!node) return
    const next = Array.from(node.children).map(
      (child) => (child as HTMLElement).getBoundingClientRect().width
    )
    setWidths((prev) =>
      prev.length === next.length && prev.every((width, i) => width === next[i])
        ? prev
        : next
    )
    if (overflowMeasureRef.current) {
      setOverflowW(overflowMeasureRef.current.getBoundingClientRect().width)
    }
  }

  // Re-measure when the unit set changes (before paint, so a new tab never
  // flashes past the edge)…
  useLayoutEffect(measureChips, [chipSignature])

  // …and whenever the strip or a chip resizes: a window resize, the sidebar
  // width change, a group folding shut, a support subject that arrives late.
  useEffect(() => {
    const container = containerRef.current
    const measure = measureRef.current
    if (!container || !measure) return
    const update = () => {
      setAvailable(container.getBoundingClientRect().width)
      const columnGap = Number.parseFloat(getComputedStyle(container).columnGap)
      if (Number.isFinite(columnGap)) setGap(columnGap)
      measureChips()
    }
    update()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(update)
    observer.observe(container)
    observer.observe(measure)
    return () => observer.disconnect()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasChips])

  const activeIndex = units.findIndex(
    (unit) => unit.kind === `chip` && unit.chip.key === activeKey
  )
  // Only the UNFOLDED chips are packed: a group's mark and its chevron are
  // always drawn (a mark past the break would strand its whole group, and the
  // chevron is what makes room), and a folded chip is mounted at zero width.
  const packedUnit = (unit: StripUnit) =>
    unit.kind === `chip` && !unit.chip.folded
  const visible = useMemo(() => {
    if (widths.length !== units.length || available === 0) {
      return units.map((_, index) => index)
    }
    return partitionUnits(
      units.map((unit, index) => ({
        packed: packedUnit(unit),
        width: widths[index]!,
      })),
      available,
      gap,
      overflowW,
      activeIndex < 0 ? null : activeIndex
    )
    // units are rebuilt every render; their identity is `chipSignature`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widths, available, gap, overflowW, activeIndex, chipSignature])
  const visibleSet = new Set(visible)
  // A folded chip is mounted at zero width, never a "+N" row.
  const hidden = units.flatMap((unit, index) =>
    packedUnit(unit) && !visibleSet.has(index)
      ? [(unit as { chip: ChipModel }).chip]
      : []
  )

  const close = (keys: string[]) =>
    updateWorkTabs(teamId, (current) => closeTabs(current, keys))

  const open = (tab: WorkTab) => {
    const href = tabHref(teamSlug, tab, (issueId) => {
      const issue = issuesById.get(issueId)
      const boardSlug = issue ? boardSlugById.get(issue.boardId) : undefined
      return issue && boardSlug
        ? { boardSlug, identifier: issue.identifier }
        : null
    })
    if (href) void navigate(href as never)
  }

  if (units.length === 0) return null

  // Close others / Close all never reach a live chip (`closeTabs` enforces it
  // too — this only keeps the menu honest).
  const closableKeys = chips
    .filter((chip) => !chip.live)
    .map((chip) => chip.key)
  const renderChip = (chip: ChipModel, interactive: boolean) => {
    const active = chip.key === activeKey
    const reachable = interactive && !chip.folded
    // EXP-907: the browser gesture — a MIDDLE click anywhere on a tab closes
    // it, exactly like its ×. The mousedown is swallowed too, or Chrome opens
    // its autoscroll cursor (and a middle click on the label's anchor role
    // would try a new tab) before the auxclick ever lands. A live chip has no
    // × and takes no middle click either (EXP-877).
    const closable = reachable && !chip.live
    const body = (
      <div
        key={chip.key}
        data-testid={reachable ? `work-tab-${chip.key}` : undefined}
        data-active={active || undefined}
        onMouseDown={
          closable
            ? (event) => {
                if (event.button === 1) event.preventDefault()
              }
            : undefined
        }
        onAuxClick={
          closable
            ? (event) => {
                if (event.button !== 1) return
                event.preventDefault()
                event.stopPropagation()
                close([chip.key])
              }
            : undefined
        }
        className={cn(
          `group/tab flex h-8 max-w-[15rem] shrink-0 items-center rounded-md border border-transparent`,
          active
            ? `border-glass-stroke-card bg-glass-active text-foreground`
            : `text-muted-foreground hover:bg-glass-active hover:text-foreground`
        )}
      >
        <Button
          variant="ghost"
          size="sm"
          tabIndex={reachable ? undefined : -1}
          // EXP-905: a live chip has no × — its label gets the same room on
          // the right as on the left instead of touching the border.
          className={cn(
            `h-8 min-w-0 flex-1 justify-start gap-1.5 bg-transparent! pl-2 font-normal`,
            chip.live ? `pr-2` : `pr-1`
          )}
          onClick={reachable ? () => open(chip.tab) : undefined}
          aria-current={active ? `page` : undefined}
        >
          <span className="flex size-3.5 shrink-0 items-center justify-center">
            {chip.lead}
          </span>
          {chip.identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {chip.identifier}
            </span>
          )}
          <span className="min-w-0 truncate text-sm">
            {chip.tab.kind === `support` ? (
              <SupportTitle threadId={chip.tab.threadId} />
            ) : (
              chip.title
            )}
          </span>
        </Button>
        {/* EXP-877: a live run's tab is permanent — it has no close button at
            all, and its context menu offers no Close. */}
        {!chip.live && (
          <Button
            variant="ghost"
            size="icon-xs"
            tabIndex={reachable ? undefined : -1}
            className="mr-1 shrink-0"
            aria-label="Close tab"
            onClick={reachable ? () => close([chip.key]) : undefined}
          >
            <UiCloseIcon />
          </Button>
        )}
      </div>
    )
    // The fold: a grid track that slides from 1fr to 0fr, so the chips of a
    // folded group take no width at all (and never reach the "+N" menu).
    const wrapped = (
      <div
        key={chip.key}
        aria-hidden={chip.folded || undefined}
        inert={chip.folded || undefined}
        className={cn(
          `grid min-w-0 shrink-0 overflow-hidden transition-[grid-template-columns,opacity] duration-standard ease-standard motion-reduce:transition-none`,
          chip.folded ? `grid-cols-[0fr] opacity-0` : `grid-cols-[1fr] opacity-100`
        )}
      >
        <div className="min-w-0">{body}</div>
      </div>
    )
    if (!reachable) return wrapped
    return (
      <ContextMenu key={chip.key}>
        <ContextMenuTrigger asChild>{wrapped}</ContextMenuTrigger>
        <ContextMenuContent>
          {!chip.live && (
            <ContextMenuItem onSelect={() => close([chip.key])}>
              Close
            </ContextMenuItem>
          )}
          <ContextMenuItem
            disabled={closableKeys.filter((key) => key !== chip.key).length === 0}
            onSelect={() => close(closableKeys.filter((key) => key !== chip.key))}
          >
            Close others
          </ContextMenuItem>
          <ContextMenuItem
            disabled={closableKeys.length === 0}
            onSelect={() => close(closableKeys)}
          >
            Close all
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    )
  }

  const renderUnit = (unit: StripUnit, index: number, interactive: boolean) => {
    if (unit.kind === `chip`) return renderChip(unit.chip, interactive)
    if (unit.kind === `group`) {
      return (
        <Button
          key={`group:${unit.agent}`}
          variant="ghost"
          size="icon-xs"
          tabIndex={interactive ? undefined : -1}
          className="shrink-0"
          aria-expanded={!unit.collapsed}
          aria-label={`${agentLabel(unit.agent)} runs`}
          title={`${agentLabel(unit.agent)} runs`}
          onClick={
            interactive
              ? () => setTabGroupCollapsed(teamId, unit.agent, !unit.collapsed)
              : undefined
          }
        >
          <AgentBrandMark agent={unit.agent} className="size-3.5" />
        </Button>
      )
    }
    return (
      <Button
        key={`chevron:${unit.agent}:${index}`}
        variant="ghost"
        size="icon-xs"
        tabIndex={interactive ? undefined : -1}
        className="shrink-0 text-muted-foreground"
        aria-label="Collapse"
        title="Collapse"
        onClick={
          interactive
            ? () => setTabGroupCollapsed(teamId, unit.agent, true)
            : undefined
        }
      >
        <UiChevronLeftIcon />
      </Button>
    )
  }

  return (
    <div
      className="relative flex h-full min-w-0 flex-1 items-center"
      data-testid="work-tabs-strip"
    >
      {/* The measurement row: every unit at its natural width, invisible and
          out of the tab order and the accessibility tree. */}
      <div
        ref={measureRef}
        aria-hidden
        inert
        className="pointer-events-none invisible absolute top-0 left-0 flex w-max gap-1"
      >
        {units.map((unit, index) => renderUnit(unit, index, false))}
      </div>
      <Button
        ref={overflowMeasureRef}
        aria-hidden
        tabIndex={-1}
        variant="ghost"
        size="sm"
        className="pointer-events-none invisible absolute top-0 left-0 px-2"
      >
        +{Math.max(chips.length, 1)}
      </Button>

      <div ref={containerRef} className="flex min-w-0 flex-1 items-center gap-1">
        {units.map((unit, index) =>
          visibleSet.has(index) ? renderUnit(unit, index, true) : null
        )}
        {hidden.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="shrink-0 px-2 text-muted-foreground"
                aria-label={`${hidden.length} more tabs`}
              >
                +{hidden.length}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-72">
              {hidden.map((chip) => (
                <DropdownMenuItem key={chip.key} onClick={() => open(chip.tab)}>
                  <span className="flex size-3.5 shrink-0 items-center justify-center">
                    {chip.lead}
                  </span>
                  {chip.identifier && (
                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                      {chip.identifier}
                    </span>
                  )}
                  <span className="min-w-0 truncate">
                    {chip.tab.kind === `support` ? (
                      <SupportTitle threadId={chip.tab.threadId} />
                    ) : (
                      chip.title
                    )}
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
    </div>
  )
}

/** A chip's lead glyph: a live run's state dot (the EXP-848 ping while the
 *  agent works), else the issue's status glyph, else a muted dot for an ended
 *  run that has no issue behind it. */
function ChipLead({
  tab,
  run,
  issue,
}: {
  tab: WorkTab
  run: CodingSession | undefined
  issue: Issue | undefined
}) {
  const live = tab.kind !== `support` && tab.live && run && run.status !== `ended`
  if (live) {
    const prState = issue?.prState ?? run.prState
    return (
      <RunningIndicator
        state={sessionDisplayState(run, prState)}
        working={sessionRowIsWorking(run, prState)}
      />
    )
  }
  if (issue) {
    return <IssueStatusIcon issue={issue} className="size-3.5!" />
  }
  return <LiveDot tone={LIVE_DOT_TONE_BY_SESSION_TONE.muted} />
}
