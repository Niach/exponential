import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import type * as React from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue } from "@/db/schema"
import { issueCollection, sessionCollection } from "@/lib/collections"
import {
  AgentRunMark,
  conceptIcon,
  Button,
  LiveDot,
  Menu,
  MenuGestureHost,
  menuProps,
  StripChip,
  StripOverflowButton,
  type MenuSessionProps,
} from "@exp/ui"
import {
  closeTabs,
  isNewRunPath,
  NEW_RUN_TAB_KEY,
  partitionTabs,
  routePathFromLocation,
  tabHref,
  tabKey,
  type WorkTab,
} from "@/lib/work-tabs"
import {
  openNewRunPage,
  updateWorkTabs,
  useWorkTabs,
} from "@/hooks/use-work-tabs"
import { useAgents, useSessionTitles, useThreadsList } from "@/hooks/use-session-tree"
import { useSession } from "@/hooks/use-session"
import {
  sessionState,
  toTreeSession,
  type SessionRow,
  type ThreadsListRow,
} from "@/lib/session-tree"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { ThreadRunMark, threadRunMarkState } from "@/components/team/sidebar-running"
import { AgentIcon } from "@/components/team/sidebar-agents"

// EXP-870: the WORK TABS strip — browser-like tabs on the bare ground above
// the content card, md+ only (the desktop's title-band strip; phones keep
// their tab bar). The model is `lib/work-tabs.ts`, the writer
// `work-tabs-sync.tsx` (route-driven, EXP-1293); this only draws it:
//
//   * a FLAT chip list in stored order: issues (status glyph + identifier +
//     title), threads (the agent mark — the spark while it works, amber
//     while it needs you — + the thread title), agents' channels (the
//     agent's icon in its color + its name) and raw sessions (the mark +
//     the session's name);
//   * the ACTIVE chip is derived from the URL — never stored;
//   * every chip closes — its ×, a middle click, and Close / Close others /
//     Close all all reach it;
//   * chips that do not fit collapse into a trailing "+N" menu
//     (`partitionTabs`, the contract's, measured against the real chips);
//   * EXP-1334: every chip is THE strip chip (`StripChip`), the thread
//     header's chip box — one tab component for both strips;
//   * EXP-1255: a chip's × shows only while the tab is hovered (or the × has
//     keyboard focus); a ghost "+" after the last tab opens the New session
//     tab — the home composer (`openNewRunPage`, Alt/Option+T too).
//     Switching tabs never animates.

/** The work-tab chips' context-menu kind (`menuProps`). */
const TAB_MENU_KIND = `work-tab`

const UiCloseIcon = conceptIcon(`ui-close`)
const UiAddIcon = conceptIcon(`ui-add`)
// The New session tab's lead: the Agent entry's concept (the sidebar's).
const NewRunIcon = conceptIcon(`action-chat`)

/** EXP-1255: the New session chip's title. */
export const NEW_RUN_TAB_TITLE = `New session`

interface ChipModel {
  key: string
  tab: WorkTab
  identifier: string | null
  /** null = the tab's subject has not loaded yet (D8: a placeholder bar). */
  title: string | null
  lead: React.ReactNode
}

/** A chip's title, or the skeleton's short muted bar while its subject loads
 *  (D8: never the text "Loading…"). */
function ChipTitle({ title }: { title: string | null }) {
  if (title !== null) return <>{title}</>
  return (
    <span
      aria-hidden
      data-testid="work-tab-title-placeholder"
      className="inline-block h-2 w-16 rounded bg-muted align-middle"
    />
  )
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

  const issueIds = useMemo(
    () =>
      [
        ...new Set(
          state.tabs.flatMap((tab) => (tab.kind === `issue` ? [tab.issueId] : []))
        ),
      ].sort(),
    [state.tabs]
  )
  const sessionIds = useMemo(
    () =>
      [
        ...new Set(
          state.tabs.flatMap((tab) => (tab.kind === `session` ? [tab.sessionId] : []))
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
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      sessionIds.length > 0
        ? query
            .from({ s: sessionCollection })
            .where(({ s }) => inArray(s.id, sessionIds))
        : undefined,
    [sessionIds.join(`,`)]
  )
  const issuesById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((row) => [row.id, row])),
    [issueRows]
  )
  const sessionList = useMemo(() => (sessionRows ?? []) as SessionRow[], [sessionRows])
  const sessionsById = useMemo(
    () => new Map(sessionList.map((row) => [row.id, row])),
    [sessionList]
  )
  // EXP-1341: a session tab reads THE display title (`sessionTitle`); a
  // held one (its shapes still loading) shows the placeholder.
  const { titles: sessionTitles } = useSessionTitles(sessionList)
  // Thread chips read their title + state off the Threads list (the ONE
  // model, `partitionThreads`); agent chips off the Agents list. Neither
  // query runs while no such tab is open.
  const hasThreadTabs = state.tabs.some((tab) => tab.kind === `thread`)
  const hasAgentTabs = state.tabs.some((tab) => tab.kind === `agent`)
  const { groups } = useThreadsList(hasThreadTabs ? teamId : null)
  const threadsById = useMemo(() => {
    const map = new Map<string, { row: ThreadsListRow; live: boolean }>()
    for (const group of groups) {
      for (const row of group.rows) {
        map.set(row.sessionId, { row, live: group.key !== `recent` })
      }
    }
    return map
  }, [groups])
  const { data: authSession } = useSession()
  const { agents } = useAgents(hasAgentTabs ? teamId : null, authSession?.user?.id)
  const agentsById = useMemo(
    () => new Map(agents.map((agent) => [agent.agentId, agent])),
    [agents]
  )
  const boardSlugById = useMemo(
    () => new Map((boards ?? []).map((board) => [board.id, board.slug])),
    [boards]
  )

  const tabs = state.tabs

  // The active tab, off the URL alone.
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const activeKey = useMemo(() => {
    if (isNewRunPath(pathname)) {
      return tabs.some((tab) => tab.kind === `new`) ? NEW_RUN_TAB_KEY : null
    }
    const path = routePathFromLocation(pathname, null)
    if (!path) return null
    const match = tabs.find((tab) => {
      switch (path.kind) {
        case `thread`:
          return tab.kind === `thread` && tab.threadId === path.threadId
        case `agent`:
          return tab.kind === `agent` && tab.agentId === path.agentId
        case `session`:
          return tab.kind === `session` && tab.sessionId === path.sessionId
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

  const chipOf = (tab: WorkTab): ChipModel => {
    const key = tabKey(tab)
    switch (tab.kind) {
      case `new`:
        return {
          key,
          tab,
          identifier: null,
          title: NEW_RUN_TAB_TITLE,
          lead: <NewRunIcon className="size-3.5 text-muted-foreground" />,
        }
      case `thread`: {
        const entry = threadsById.get(tab.threadId)
        return {
          key,
          tab,
          identifier: null,
          title: entry ? entry.row.title : null,
          lead: !entry ? (
            <LiveDot tone="muted" />
          ) : entry.live ? (
            <ThreadRunMark row={entry.row} ringClassName="ring-background" />
          ) : (
            <AgentRunMark
              agent={sessionCollection.get(tab.threadId)?.cli ?? null}
              state="ended"
            />
          ),
        }
      }
      case `agent`: {
        const agent = agentsById.get(tab.agentId)
        return {
          key,
          tab,
          identifier: null,
          title: agent ? agent.name : null,
          lead: agent ? (
            <AgentIcon agent={agent} className="size-3.5" />
          ) : (
            <LiveDot tone="muted" />
          ),
        }
      }
      case `session`: {
        const row = sessionsById.get(tab.sessionId)
        return {
          key,
          tab,
          identifier: null,
          title: row ? (sessionTitles.get(row.id) ?? null) : null,
          lead: row ? <SessionMark row={row} /> : <LiveDot tone="muted" />,
        }
      }
      case `issue`: {
        const issue = issuesById.get(tab.issueId)
        return {
          key,
          tab,
          identifier: issue?.identifier ?? null,
          title: issue?.title ?? null,
          lead: issue ? (
            <IssueStatusIcon issue={issue} className="size-3.5!" />
          ) : (
            <LiveDot tone="muted" />
          ),
        }
      }
    }
  }

  const chips = tabs.map(chipOf)

  // ── Overflow: measure every chip in a hidden row, pack the visible ones.
  const containerRef = useRef<HTMLDivElement>(null)
  const measureRef = useRef<HTMLDivElement>(null)
  const overflowMeasureRef = useRef<HTMLButtonElement>(null)
  const plusRef = useRef<HTMLButtonElement>(null)
  const [available, setAvailable] = useState(0)
  const [widths, setWidths] = useState<number[]>([])
  const [overflowW, setOverflowW] = useState(40)
  // `gap-1` is rem-based and the md+ root font is 1.15625rem, so the gap is
  // measured too rather than assumed.
  const [gap, setGap] = useState(4)
  const chipSignature = chips
    .map((chip) => `${chip.key}|${chip.title}`)
    .join(`\n`)

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
  // width change.
  useEffect(() => {
    const container = containerRef.current
    const measure = measureRef.current
    if (!container || !measure) return
    const update = () => {
      const columnGap = Number.parseFloat(getComputedStyle(container).columnGap)
      const gapPx = Number.isFinite(columnGap) ? columnGap : 0
      if (Number.isFinite(columnGap)) setGap(columnGap)
      // EXP-1255: the "+" always shows, so its slot never goes to a chip.
      const plus = plusRef.current?.getBoundingClientRect().width ?? 0
      setAvailable(container.getBoundingClientRect().width - (plus > 0 ? plus + gapPx : 0))
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

  const openNewRun = () => openNewRunPage(teamId, teamSlug, navigate)

  const allKeys = chips.map((chip) => chip.key)
  const renderChip = (chip: ChipModel, interactive: boolean) => {
    const active = chip.key === activeKey
    // EXP-907: the browser gesture — a MIDDLE click anywhere on a tab closes
    // it, exactly like its ×. The mousedown is swallowed too, or Chrome opens
    // its autoscroll cursor (and a middle click on the label's anchor role
    // would try a new tab) before the auxclick ever lands.
    // EXP-1334: THE strip chip (`@exp/ui` `StripChip`), the thread header's
    // chip box: lead · mono identifier · title, the × as its trailing part.
    const body = (
      <StripChip
        key={chip.key}
        active={active}
        lead={chip.lead}
        identifier={chip.identifier}
        label={<ChipTitle title={chip.title} />}
        tabIndex={interactive ? undefined : -1}
        aria-current={active ? `page` : undefined}
        onClick={interactive ? () => open(chip.tab) : undefined}
        boxClassName="group/tab shrink-0"
        boxProps={{
          "data-testid": interactive ? `work-tab-${chip.key}` : undefined,
          // A right-click (or long-press) opens the tab menu through the
          // strip's gesture host, the way issue rows open theirs.
          ...(interactive ? menuProps(TAB_MENU_KIND, chip.key) : {}),
          onMouseDown: interactive
            ? (event) => {
                if (event.button === 1) event.preventDefault()
              }
            : undefined,
          onAuxClick: interactive
            ? (event) => {
                if (event.button !== 1) return
                event.preventDefault()
                event.stopPropagation()
                close([chip.key])
              }
            : undefined,
        }}
        trailing={
          <Button
            variant="ghost"
            size="icon-xs"
            tabIndex={interactive ? undefined : -1}
            // EXP-1255: hover-only, every tab; keyboard focus shows it too.
            className="mr-1 shrink-0 opacity-0 group-hover/tab:opacity-100 focus-visible:opacity-100"
            aria-label="Close tab"
            onClick={interactive ? () => close([chip.key]) : undefined}
          >
            <UiCloseIcon />
          </Button>
        }
      />
    )
    return body
  }

  const tabMenu = ({ target, open: menuOpen, onOpenChange }: MenuSessionProps) => {
    const others = allKeys.filter((key) => key !== target.id)
    return (
      <Menu
        mode="pointer"
        anchor={target.anchor}
        open={menuOpen}
        onOpenChange={onOpenChange}
        returnFocus={target.origin}
        aria-label="Tab actions"
        title="Tab"
        entries={[
          { kind: `item`, id: `close`, label: `Close`, onSelect: () => close([target.id]) },
          {
            kind: `item`,
            id: `close-others`,
            label: `Close others`,
            disabled: others.length === 0,
            onSelect: () => close(others),
          },
          { kind: `item`, id: `close-all`, label: `Close all`, onSelect: () => close(allKeys) },
        ]}
      />
    )
  }

  return (
    <MenuGestureHost menus={{ [TAB_MENU_KIND]: tabMenu }}>
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
        <StripOverflowButton
          ref={overflowMeasureRef}
          aria-hidden
          tabIndex={-1}
          count={Math.max(chips.length, 1)}
          className="pointer-events-none invisible absolute top-0 left-0"
        />

        <div ref={containerRef} className="flex min-w-0 flex-1 items-center gap-1">
          {chips.map((chip, index) =>
            visibleSet.has(index) ? renderChip(chip, true) : null
          )}
          {hidden.length > 0 && (
            <Menu
              align="end"
              aria-label="More tabs"
              title="Tabs"
              contentClassName="w-72"
              entries={hidden.map((chip) => ({
                kind: `item` as const,
                id: chip.key,
                icon: (
                  <span className="flex size-3.5 shrink-0 items-center justify-center">
                    {chip.lead}
                  </span>
                ),
                label: (
                  <>
                    {chip.identifier && (
                      <span className="mr-1.5 font-mono text-xs text-muted-foreground">
                        {chip.identifier}
                      </span>
                    )}
                    <ChipTitle title={chip.title} />
                  </>
                ),
                onSelect: () => open(chip.tab),
              }))}
              trigger={
                <StripOverflowButton
                  count={hidden.length}
                  aria-label={`${hidden.length} more tabs`}
                />
              }
            />
          )}
          <Button
            ref={plusRef}
            variant="ghost"
            size="icon-xs"
            className="shrink-0 text-muted-foreground hover:text-foreground"
            aria-label={NEW_RUN_TAB_TITLE}
            title={NEW_RUN_TAB_TITLE}
            data-testid="work-tabs-new-run"
            onClick={openNewRun}
          >
            <UiAddIcon />
          </Button>
        </div>
      </div>
    </MenuGestureHost>
  )
}

/** A raw session's mark: the working spark, amber while it needs you, the
 *  dimmed brand mark once it ended. */
function SessionMark({ row }: { row: SessionRow }) {
  const state = sessionState(toTreeSession(row))
  return (
    <AgentRunMark
      agent={row.cli}
      state={
        state === `done`
          ? `ended`
          : threadRunMarkState({
              needsYou: state === `needs_you`,
              busy: state === `working`,
            })
      }
      ringClassName="ring-background"
    />
  )
}
