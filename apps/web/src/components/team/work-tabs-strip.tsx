import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import type * as React from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, CodingSession, Issue } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { conceptIcon } from "@/lib/icons.generated"
import {
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
import { SESSION_DOT_CLASS } from "@/lib/session-dot"
import { sessionIdentity } from "@/lib/session-identity"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import {
  closeTabs,
  liveSig,
  orderedTabs,
  partitionTabs,
  routePathFromLocation,
  tabHref,
  tabKey,
  type WorkTab,
} from "@/lib/work-tabs"
import { updateWorkTabs, useWorkTabs } from "@/hooks/use-work-tabs"
import { RunningIndicator } from "@/components/agent-session-row"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// EXP-870: the WORK TABS strip — browser-like tabs on the bare ground above
// the content card, md+ only (the desktop's title-band strip; phones keep
// their tab bar). The model is `lib/work-tabs.ts`, the writer
// `work-tabs-sync.tsx`; this only draws it:
//
//   * chips in `orderedTabs` order (live runs first), the ACTIVE one derived
//     from the URL — never stored;
//   * a chip's lead is the run's state (amber waiting on you, green PR open,
//     the ping while the agent works) or, without a live run, the issue's
//     status glyph / a muted dot;
//   * right-click: Close / Close others / Close all — closing a live run's tab
//     hides it until its state changes and NEVER stops the run;
//   * chips that do not fit collapse into a trailing "+N" menu
//     (`partitionTabs`, measured against the real chips).

const UiCloseIcon = conceptIcon(`ui-close`)
const NavSupportIcon = conceptIcon(`nav-support`)

interface ChipModel {
  key: string
  tab: WorkTab
  identifier: string | null
  title: string
  lead: React.ReactNode
}

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
  const tabs = useMemo(() => orderedTabs(state.tabs), [state.tabs])

  const issueIds = useMemo(
    () =>
      [...new Set(tabs.flatMap((tab) => (tab.kind === `issue` ? [tab.issueId] : [])))].sort(),
    [tabs]
  )
  const runIds = useMemo(
    () =>
      [
        ...new Set(
          tabs.flatMap((tab) =>
            tab.kind !== `support` && tab.runId ? [tab.runId] : []
          )
        ),
      ].sort(),
    [tabs]
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

  const chips: ChipModel[] = tabs.map((tab) => {
    const key = tabKey(tab)
    if (tab.kind === `support`) {
      return {
        key,
        tab,
        identifier: null,
        title: ``,
        lead: <NavSupportIcon className="size-3.5 text-muted-foreground" />,
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
    }
  })

  // ── Overflow: measure every chip in a hidden row, pack the visible ones.
  const containerRef = useRef<HTMLDivElement>(null)
  const measureRef = useRef<HTMLDivElement>(null)
  const overflowMeasureRef = useRef<HTMLButtonElement>(null)
  const [available, setAvailable] = useState(0)
  const [widths, setWidths] = useState<number[]>([])
  const [overflowW, setOverflowW] = useState(40)
  // `gap-1` is rem-based and the md+ root font is 1.15625rem, so the gap is
  // measured too rather than assumed.
  const [gap, setGap] = useState(4)
  const chipSignature = chips.map((chip) => `${chip.key}|${chip.title}`).join(`\n`)

  const hasChips = chips.length > 0
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

  // Re-measure when the chip set changes (before paint, so a new tab never
  // flashes past the edge)…
  useLayoutEffect(measureChips, [chipSignature])

  // …and whenever the strip or a chip resizes: a window resize, the sidebar
  // width change, a support subject that arrives late.
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

  const activeIndex = chips.findIndex((chip) => chip.key === activeKey)
  const visible = useMemo(() => {
    if (widths.length !== chips.length || available === 0) {
      return chips.map((_, index) => index)
    }
    return partitionTabs(
      widths,
      available,
      gap,
      overflowW,
      activeIndex < 0 ? null : activeIndex
    )
    // chips are rebuilt every render; their identity is `chipSignature`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widths, available, gap, overflowW, activeIndex, chipSignature])
  const visibleSet = new Set(visible)
  const hidden = chips.filter((_, index) => !visibleSet.has(index))

  const sigOf = (runId: string) => {
    const run = runsById.get(runId)
    return run && run.status !== `ended` ? liveSig(run, new Date()) : null
  }
  const close = (keys: string[]) =>
    updateWorkTabs(teamId, (current) => closeTabs(current, keys, sigOf))

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

  if (chips.length === 0) return null

  const allKeys = chips.map((chip) => chip.key)
  const renderChip = (chip: ChipModel, interactive: boolean) => {
    const active = chip.key === activeKey
    const body = (
      <div
        key={chip.key}
        data-testid={interactive ? `work-tab-${chip.key}` : undefined}
        data-active={active || undefined}
        className={cn(
          `group/tab flex h-8 max-w-[15rem] shrink-0 items-center rounded-md border border-transparent`,
          active
            ? `border-glass-stroke-card bg-glass-panel text-foreground`
            : `text-muted-foreground hover:bg-glass-active hover:text-foreground`
        )}
      >
        <Button
          variant="ghost"
          size="sm"
          tabIndex={interactive ? undefined : -1}
          className="h-8 min-w-0 flex-1 justify-start gap-1.5 bg-transparent! pr-1 pl-2 font-normal"
          onClick={interactive ? () => open(chip.tab) : undefined}
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
        <Button
          variant="ghost"
          size="icon-xs"
          tabIndex={interactive ? undefined : -1}
          className={cn(
            `mr-1 shrink-0`,
            !active && `opacity-0 group-hover/tab:opacity-100 focus-visible:opacity-100`
          )}
          aria-label="Close tab"
          onClick={interactive ? () => close([chip.key]) : undefined}
        >
          <UiCloseIcon />
        </Button>
      </div>
    )
    if (!interactive) return body
    return (
      <ContextMenu key={chip.key}>
        <ContextMenuTrigger asChild>{body}</ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onSelect={() => close([chip.key])}>Close</ContextMenuItem>
          <ContextMenuItem
            disabled={chips.length < 2}
            onSelect={() => close(allKeys.filter((key) => key !== chip.key))}
          >
            Close others
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => close(allKeys)}>Close all</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    )
  }

  return (
    <div
      className="relative flex h-full min-w-0 flex-1 items-center"
      data-testid="work-tabs-strip"
    >
      {/* The measurement row: every chip at its natural width, invisible and
          out of the tab order and the accessibility tree. */}
      <div
        ref={measureRef}
        aria-hidden
        inert
        className="pointer-events-none invisible absolute top-0 left-0 flex w-max gap-1"
      >
        {chips.map((chip) => renderChip(chip, false))}
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
        {chips.map((chip, index) =>
          visibleSet.has(index) ? renderChip(chip, true) : null
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

/** A chip's lead glyph (the desktop's `live_chip_state` precedence): a live
 *  run waiting on you amber, with its PR open green, working the ping; else
 *  the issue's status glyph, a run tab's steady dot (muted once it ended). */
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
    const sig = liveSig(run, new Date())
    if (sig.attention) {
      return <span className={`size-2 rounded-full ${SESSION_DOT_CLASS.needs_input}`} />
    }
    if (sig.review) {
      return <span className={`size-2 rounded-full ${SESSION_DOT_CLASS.review}`} />
    }
    const prState = issue?.prState ?? run.prState
    if (sessionRowIsWorking(run, prState)) {
      return (
        <RunningIndicator
          state={sessionDisplayState(run, prState)}
          working
        />
      )
    }
  }
  if (issue) {
    return <IssueStatusIcon issue={issue} className="size-3.5!" />
  }
  return (
    <span
      className={`size-2 rounded-full ${
        live ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS.muted
      }`}
    />
  )
}
