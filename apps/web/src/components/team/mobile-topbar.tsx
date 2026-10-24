import { APPS_COPY } from "@exp/domain-contract/apps"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useState } from "react"
import { useMatchRoute, useNavigate, useParams } from "@tanstack/react-router"
import { conceptIcon, BoardGlyph, FabButton, FAB_CHROME_CLASS } from "@exp/ui"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { BoardSwitcherSheet } from "@/components/team/board-switcher-sheet"
import { useIssueSearch } from "@/hooks/use-issue-search"
import {
  resolveBoardTarget,
  useMobileChromeVisible,
} from "@/components/team/mobile-tab-bar"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const SwitcherChevronIcon = conceptIcon(`ui-chevron-down`)
const NavSearchIcon = conceptIcon(`nav-search`)
const UiBackIcon = conceptIcon(`ui-back`)

interface TeamMobileTopbarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
}

/** Which phone header a route wears (chats.json `shell`): the board
 *  header, none (the page draws its own headline: Chats, the Team tab), or
 *  a titled bar (a pushed page from the Team tab gets a back to it). */
export type MobileTopbarKind =
  | { kind: `board` }
  | { kind: `none` }
  | { kind: `title`; title: string; backToTeam: boolean }

// Mobile-only chrome (EXP-189), mirroring the native apps' headers. Thread
// 58cb32df (chats.json): the BOARD header = the board switcher pill + the
// search circle (the settings gear and the avatar menu moved to the Team
// tab); Chats and the Team tab draw their own large headline, so no bar
// there; the other top-level surfaces keep a titled bar, and the pages the
// Team tab pushes (settings, apps) get a back to it. Primary navigation
// lives in the floating MobileTabBar; the whole header is `md:hidden` and
// hides with it on detail routes (which carry their own headers).
export function TeamMobileTopbar({
  teamSlug,
  team,
  boards,
}: TeamMobileTopbarProps) {
  const navigate = useNavigate()
  const matchRoute = useMatchRoute()
  const visible = useMobileChromeVisible()
  const { boardSlug } = useParams({ strict: false })
  const [switcherOpen, setSwitcherOpen] = useState(false)
  const issueSearch = useIssueSearch()

  const on = (to: string) => Boolean(matchRoute({ to, fuzzy: true }))
  const bar: MobileTopbarKind =
    on(`/t/$teamSlug/threads`) || on(`/t/$teamSlug/team`)
      ? { kind: `none` }
      : on(`/t/$teamSlug/devices`)
        ? { kind: `title`, title: CHATS_COPY.tabs.devices, backToTeam: false }
        : on(`/t/$teamSlug/agent`)
          ? { kind: `title`, title: `Agent`, backToTeam: false }
          : on(`/t/$teamSlug/apps`)
            ? { kind: `title`, title: APPS_COPY.title, backToTeam: true }
            : on(`/t/$teamSlug/settings`)
              ? { kind: `title`, title: `Settings`, backToTeam: true }
              : { kind: `board` }

  if (!visible || bar.kind === `none`) return null

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  const switcherLabel = boardTarget?.name ?? team?.name ?? teamSlug

  if (bar.kind === `title`) {
    return (
      <header className="sticky top-0 z-30 flex h-12 items-center gap-1 border-b border-border/60 px-3 glass-chrome-top md:hidden">
        {bar.backToTeam && (
          <button
            type="button"
            aria-label={CHATS_COPY.tabs.team}
            onClick={() => void navigate({ to: `/t/$teamSlug/team`, params: { teamSlug } })}
            className="-ml-1 flex size-9 items-center justify-center rounded-full text-muted-foreground hover:text-foreground"
          >
            <UiBackIcon className="size-4" />
          </button>
        )}
        <span className="truncate text-sm font-medium">{bar.title}</span>
      </header>
    )
  }

  return (
    <header
      data-testid="mobile-board-header"
      className="sticky top-0 z-30 flex items-center gap-3 px-4 pb-2 pt-[max(0.75rem,env(safe-area-inset-top))] glass-chrome-top md:hidden"
    >
      {/* The board switcher PILL (name + chevron → bottom sheet). EXP-1210:
          no live dot here; the per-team dots live in the sheet. */}
      <button
        type="button"
        onClick={() => setSwitcherOpen(true)}
        aria-label="Switch board"
        className={cn(
          `flex h-12 min-w-0 items-center gap-2.5 rounded-full pl-4 pr-3.5 text-base font-medium text-foreground transition-colors active:bg-glass-active`,
          FAB_CHROME_CLASS
        )}
      >
        {boardTarget && <BoardGlyph board={boardTarget} className="size-[18px]" />}
        <span className="truncate">{switcherLabel}</span>
        <SwitcherChevronIcon className="size-4 shrink-0 text-muted-foreground" />
      </button>
      <div className="ml-auto flex items-center gap-2">
        {/* EXP-686 → polish: on a board, Search sits in the header cluster
            (the natives' Issues header), not on a row of its own. */}
        {boardSlug && (
          <FabButton
            aria-label="Search"
            emphasis="primary"
            className="size-12"
            onClick={issueSearch.open}
          >
            <NavSearchIcon className="size-5" />
          </FabButton>
        )}
      </div>

      <BoardSwitcherSheet
        open={switcherOpen}
        onOpenChange={setSwitcherOpen}
        teamSlug={teamSlug}
        team={team}
        boards={boards}
        activeBoardSlug={boardSlug}
      />
    </header>
  )
}
