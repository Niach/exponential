import { useCallback, useEffect, useMemo, useState } from "react"
import { Link, useNavigate, useParams, useRouterState, useSearch } from "@tanstack/react-router"
import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import {
  FAB_CLEARANCE_CLASS,
  PhoneDock,
  PhoneDockAssistant,
  PhoneDockTab,
  PhoneNewMenu,
  conceptIcon,
  type PhoneNewItem,
} from "@exp/ui"
import type { Board } from "@/db/schema"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { useSession } from "@/hooks/use-session"
import { useBoardsForTeams } from "@/hooks/use-team-data"
import { readLastVisited } from "@/lib/last-visited"
import { draftOriginFrom, newDraftNavigation } from "@/lib/issue-draft-page"
import { phonePagePlace } from "@/lib/phone-page"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"
import { useMobileChrome } from "@/hooks/use-mobile-chrome"
import { useChatsBadge } from "@/hooks/use-nav-counts"

const ChatsIcon = conceptIcon(CHATS_SHELL.tabIcons.chats as `nav-chats`)
const TeamsIcon = conceptIcon(CHATS_SHELL.tabIcons.teams as `ui-team`)
const NewIcon = conceptIcon(CHATS_SHELL.tabIcons.new as `ui-add`)
const NewOpenIcon = conceptIcon(CHATS_SHELL.newMenu.openIcon as `ui-close`)
const SettingsIcon = conceptIcon(CHATS_SHELL.tabIcons.settings as `nav-settings`)

// Bottom padding for every scroll container that sits under the docked
// tab bar, so list ends scroll clear of it. Detail routes hide
// the bar (useMobileChromeVisible) and must NOT reserve this.
// EXP-698: the bar MEASURES itself into `--tabbar-h` (its own safe-area
// padding included) instead of every scroller re-guessing the pill geometry.
// The literal covers ONLY the frames before the first measurement — a hidden
// bar publishes `0px` rather than dropping the property, so no route ever
// falls back to it while there is no pill on screen. `+1.25rem` is the gap
// above the pill.
// EXP-698 r5: the bulk bar takes the tab bar's slot on a selection, so the
// clearance is whichever bar is actually down there — `max()` of the two
// published heights, never their sum.
// #EXP-1860: the wide half — while the Assistant FAB shows, the content
// ends with a spacer the FAB's corner tall (`FAB_CLEARANCE_CLASS`, an
// `::after`: it adds to the route's own padding and is a no-op without the
// FAB), so the last row scrolls clear of the circle. Wear it on a page's
// scroller or on its content column (block or flex column).
export const PAGE_END_CLEARANCE = `max-md:pb-[calc(max(var(--tabbar-h,4.25rem),var(--bulkbar-h,0px))+1.25rem)] ${FAB_CLEARANCE_CLASS}`

// The bar shows on the fixture's pages (chats.json `shell.pages`); a detail
// carries its own back header and the bottom edge.
export function useMobileChromeVisible(): boolean {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  // #EXP-1553: a DEV-ONLY specimen page (`?dev=<id>`, the view catalog's
  // captures) is a bare board; `chat-list` keeps the bar: that specimen IS
  // the phone Chats screen.
  const { dev } = useSearch({ strict: false }) as { dev?: unknown }
  if (pathname === `/home` && dev === `chat-list`) return true
  return phonePagePlace(pathname).bar
}

// The board a team's New issue targets: the
// active board, else this device's last-used board in the team (EXP-69,
// same resolution as the team index route), else the first board.
export function resolveBoardTarget(
  teamSlug: string,
  boards: Board[] | undefined,
  activeBoardSlug: string | undefined
): Board | undefined {
  if (!boards || boards.length === 0) return undefined
  if (activeBoardSlug) {
    const active = boards.find((board) => board.slug === activeBoardSlug)
    if (active) return active
  }
  const last = readLastVisited()
  if (last?.teamSlug === teamSlug && last.boardSlug) {
    const remembered = boards.find((board) => board.slug === last.boardSlug)
    if (remembered) return remembered
  }
  return boards[0]
}

/** chats.json `shell.newMenu` Issue: the board on screen, else the last used
 *  board, else the first board of the first team; null = no board at all. */
export function newIssueBoard(input: {
  /** Every member team's boards, in team order. */
  boards: readonly Board[]
  teams: readonly { id: string; slug: string }[]
  onScreen: { teamSlug: string; boardSlug: string } | null
  last: { teamSlug: string; boardSlug?: string } | null
}): { board: Board; teamSlug: string } | null {
  const slugOf = new Map(input.teams.map((team) => [team.id, team.slug]))
  const find = (at: { teamSlug: string; boardSlug?: string } | null) =>
    at?.boardSlug
      ? input.boards.find((board) => board.slug === at.boardSlug && slugOf.get(board.teamId) === at.teamSlug)
      : undefined
  const board = find(input.onScreen) ?? find(input.last) ?? input.boards[0]
  const teamSlug = board ? slugOf.get(board.teamId) : undefined
  return board && teamSlug ? { board, teamSlug } : null
}

/** A text field inside the page has the keyboard (the Assistant tab's bar
 *  steps aside for it). */
function useTyping(enabled: boolean): boolean {
  const [typing, setTyping] = useState(false)
  useEffect(() => {
    if (!enabled) {
      setTyping(false)
      return
    }
    const read = () => {
      const field = document.activeElement
      setTyping(field instanceof HTMLElement && (field.isContentEditable || field.matches(`textarea, input`)))
    }
    read()
    const later = () => setTimeout(read, 0)
    document.addEventListener(`focusin`, read)
    document.addEventListener(`focusout`, later)
    return () => {
      document.removeEventListener(`focusin`, read)
      document.removeEventListener(`focusout`, later)
    }
  }, [enabled])
  return typing
}

// #EXP-2109 (chats.json `shell.dock`, ×3 with iOS and Android): THE phone
// bottom bar, docked: Chats · Teams · the Assistant · New (a popup: Issue,
// Chat) · Settings. No active team: the Chats badge counts every team.
// `md:hidden`: wide keeps the sidebar.
export function MobileTabBar() {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const { teamSlug, boardSlug } = useParams({ strict: false }) as { teamSlug?: string; boardSlug?: string }
  const { data: session } = useSession()
  const { teams, teamIds } = useMemberTeams(session?.user?.id)
  const { boards } = useBoardsForTeams(teamIds)
  const chats = useChatsBadge(teamIds)
  // EXP-698 r5: a multi-selection replaces the bar with the bulk bar.
  const { bulkBarPresent } = useMobileChrome()
  const { tab } = phonePagePlace(pathname)
  const onAssistant = tab === `assistant`
  const typing = useTyping(onAssistant)
  const visible = useMobileChromeVisible() && !bulkBarPresent && !typing
  const publishTabBarHeight = useChromeHeightVar(`--tabbar-h`)
  const publishDockHeight = useChromeHeightVar(`--phone-dock-h`, onAssistant)
  const publish = useCallback(
    (node: HTMLElement | null) => {
      publishTabBarHeight(node)
      publishDockHeight(node)
    },
    [publishTabBarHeight, publishDockHeight]
  )
  const [open, setOpen] = useState(false)
  useEffect(() => setOpen(false), [pathname])
  // The Assistant's composer floats above the bar (its raised disc included).
  useEffect(() => {
    const root = document.documentElement
    if (onAssistant && visible) {
      root.style.setProperty(`--phone-composer-bottom`, `calc(var(--phone-dock-h) + ${CHATS_SHELL.dock.raise + 12}px)`)
    } else root.style.removeProperty(`--phone-composer-bottom`)
    return () => {
      root.style.removeProperty(`--phone-composer-bottom`)
    }
  }, [onAssistant, visible])
  const target = useMemo(
    () =>
      newIssueBoard({
        boards,
        teams,
        onScreen: teamSlug && boardSlug ? { teamSlug, boardSlug } : null,
        last: readLastVisited(),
      }),
    // `pathname`: the last visited board moves with navigation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [boards, teams, teamSlug, boardSlug, pathname]
  )

  if (!visible) return null

  const pick = (item: PhoneNewItem) => {
    if (item === `chat`) void navigate({ to: `/home` })
    // EXP-1170: a fresh draft id at TAP time, then the New issue page.
    else if (target) {
      void navigate(
        newDraftNavigation({ teamSlug: target.teamSlug, boardId: target.board.id, from: draftOriginFrom(pathname, null) })
      )
    }
  }

  return (
    <div
      ref={publish}
      className="pointer-events-none fixed inset-x-0 bottom-0 z-[36] md:hidden"
    >
      <PhoneNewMenu open={open} onOpenChange={setOpen} onSelect={pick} disabled={{ issue: target === null }} />
      <PhoneDock>
        <PhoneDockTab
          asChild
          label={CHATS_COPY.tabs.chats}
          icon={<ChatsIcon />}
          selected={tab === `chats`}
          badge={chats}
          data-testid={CHATS_TEST_IDS.tabChats}
        >
          <Link to="/chats" />
        </PhoneDockTab>
        <PhoneDockTab
          asChild
          label={CHATS_COPY.tabs.teams}
          icon={<TeamsIcon />}
          selected={tab === `teams`}
          data-testid={CHATS_TEST_IDS.tabTeams}
        >
          <Link to="/teams" />
        </PhoneDockTab>
        <PhoneDockAssistant asChild selected={onAssistant} data-testid={CHATS_TEST_IDS.tabAssistant}>
          <Link to="/assistant" />
        </PhoneDockAssistant>
        <PhoneDockTab
          label={CHATS_COPY.tabs.new}
          icon={open ? <NewOpenIcon /> : <NewIcon />}
          selected={open}
          aria-haspopup="menu"
          aria-expanded={open}
          data-testid={CHATS_TEST_IDS.tabNew}
          onClick={() => setOpen((value) => !value)}
        />
        <PhoneDockTab
          asChild
          label={CHATS_COPY.tabs.settings}
          icon={<SettingsIcon />}
          selected={tab === `settings`}
          data-testid={CHATS_TEST_IDS.tabSettings}
        >
          <Link to="/settings" />
        </PhoneDockTab>
      </PhoneDock>
    </div>
  )
}
