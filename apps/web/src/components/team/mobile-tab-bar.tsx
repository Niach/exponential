import { useEffect, useState } from "react"
import { Link, useMatchRoute, useNavigate, useParams } from "@tanstack/react-router"
import { CHATS_COPY, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import {
  BoardGlyph,
  Button,
  FAB_CHROME_CLASS,
  PhoneBoardMenuStrip,
  PhoneTab,
  PhoneTabBar,
  TeamAvatar,
  conceptIcon,
} from "@exp/ui"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { readLastVisited } from "@/lib/last-visited"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"
import { useMobileChrome } from "@/hooks/use-mobile-chrome"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { useThreadsNav } from "@/hooks/use-nav-counts"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const ActionChatIcon = conceptIcon(`action-chat`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
// chats.json `shell.tabIcons` (Team = the active team's avatar; Board = the
// active board's glyph, this one only while the team has no board).
const BoardTabIcon = conceptIcon(`nav-issues`)
const ChatsTabIcon = conceptIcon(`nav-threads`)
const AppsTabIcon = conceptIcon(`nav-apps`)
// `shell.boardTab.caret`.
const BoardCaretIcon = conceptIcon(`nav-team-switcher`)

// Bottom padding for every scroll container that sits under the floating
// tab bar, so list ends scroll clear of the glass pill. Detail routes hide
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
export const TAB_BAR_CLEARANCE = `max-md:pb-[calc(max(var(--tabbar-h,4.25rem),var(--bulkbar-h,0px))+1.25rem)]`

// Mobile chrome (topbar + tab bar) hides on the detail routes — they carry
// their own breadcrumb/back headers, mirroring the native apps pushing a
// bar-less detail screen. Settings and the other top-level surfaces keep it.
export function useMobileChromeVisible(): boolean {
  const matchRoute = useMatchRoute()
  const onIssueDetail = matchRoute({
    to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
    fuzzy: true,
  })
  // EXP-740/EXP-739: a raw session is a full-screen view with its own back
  // header — the native apps push it bar-less too.
  const onSessionDetail = matchRoute({
    to: `/t/$teamSlug/sessions/$sessionId`,
    fuzzy: true,
  })
  // EXP-1170: the New issue page is the issue detail in draft mode.
  const onDraftPage = matchRoute({
    to: `/t/$teamSlug/drafts/$draftId`,
    fuzzy: true,
  })
  // EXP-1293: a thread (its pager under its own header) and an agent's
  // channel are details too.
  const onThreadDetail = matchRoute({
    to: `/t/$teamSlug/threads/$threadId`,
    fuzzy: true,
  })
  const onAgentChannel = matchRoute({
    to: `/t/$teamSlug/agents/$agentId`,
    fuzzy: true,
  })
  // VAPP-110: the app editor owns the bottom edge (its composer).
  const onAppEditor = matchRoute({
    to: `/t/$teamSlug/apps/$appSlug`,
    fuzzy: true,
  })
  // chats.json: the Inbox is PUSHED from the Chats header's inbox circle
  // (its own back header, no segments), so it hides the bar like a detail.
  const onInbox = matchRoute({ to: `/t/$teamSlug/inbox`, fuzzy: true })
  // The Agent page (HOME) and the Chats list keep the standard chrome.
  return (
    !onInbox &&
    !onIssueDetail &&
    !onSessionDetail &&
    !onDraftPage &&
    !onThreadDetail &&
    !onAgentChannel &&
    !onAppEditor
  )
}

// The board the Issues tab / compose FAB / topbar switcher target: the
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

// EXP-1293 → thread 58cb32df: the Chats tab's dot — amber while a chat
// needs you, green while one runs, else the unread dot while a chat holds
// an unread reply (green-500/yellow-400 = the natives' semantic tokens,
// EXP-699). Phones read the ONE active team (chats.json `shell`).
function chatsDot(nav: { live: boolean; needsYou: boolean; unread: boolean }): string | null {
  if (!nav.live && !nav.unread) return null
  return nav.needsYou ? `bg-yellow-400` : nav.live ? `bg-green-500` : `bg-primary`
}

// The detached FAB beside the nav pill: ONE 52px capsule with two 52px arms
// — Start chat | New issue — split by a hairline (`FAB_GROUP_CLASS` +
// `FAB_ARM_CLASS`). The capsule is the circle STRETCHED, so it says 52px the
// same way the circle does, never `3.25rem`. #EXP-1379: iOS metrics exactly
// (ExpUI `MobileTabBarMetrics.capsuleWidth` = 2 × 52 + the hairline): the
// capsule is 105px OUTSIDE, its 1px border inside it, the arms share the rest,
// so the tab capsule beside it gets the same width as on iOS and Android.
// EXP-973: it is the SAME capsule on every tab-bar route. EXP-827 drew it
// only on a board and left a lone chat circle everywhere else, so filing a
// bug from Devices or Threads meant a trip back to a board first. With no
// board in the team at all there is nothing to file into — the arm then dims
// (disabled, "New issue (no board)") instead of disappearing, so the control
// never moves under the thumb. Same shape ×3 (iOS / Android `composeEnabled`).
// The Agent page is the phone's FIRST screen and keeps the bar, so the chat
// arm is SELECTED there — the tabs' active circle inside the arm — the way a
// tab is lit on its own surface.
const FAB_GROUP_CLASS = `pointer-events-auto flex h-[52px] w-[105px] shrink-0 items-stretch overflow-hidden rounded-full text-foreground ${FAB_CHROME_CLASS}`
const FAB_ARM_CLASS = `relative flex min-w-0 flex-1 items-center justify-center text-foreground transition-colors active:bg-glass-active`

interface MobileTabBarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
}

// Native-parity mobile navigation (EXP-189): a floating glass capsule with
// the top-level destinations plus a detached compose FAB, replacing the old
// sidebar-as-drawer. Desktop keeps the persistent sidebar (`md:hidden`).
// Thread 58cb32df → #EXP-1384 (chats.json `shell.tabs`, ×3 with iOS and
// Android): Board · Chats · Apps · Team, each an icon over its label on
// `PhoneTabBar` (the iOS Liquid Glass look-alike on the FAB chrome recipe,
// the drag lens included). The Board tab = the active board's glyph + name;
// ON the board view it carries the caret and a tap morphs the whole strip
// into the BOARD MENU (`PhoneBoardMenuStrip`, the only phone board switch).
// The Team tab is the active team's avatar + name and holds settings,
// Devices and the account; the Inbox is in every page header. The phone
// OPENS on the Agent page (`routes/index.tsx`), whose entry is the FAB's chat
// arm.
export function MobileTabBar({
  teamSlug,
  team,
  boards,
}: MobileTabBarProps) {
  const matchRoute = useMatchRoute()
  const navigate = useNavigate()
  const permissions = useTeamPermissions(team)
  const [menuOpen, setMenuOpen] = useState(false)
  const [createBoardOpen, setCreateBoardOpen] = useState(false)
  // EXP-698 r5: a multi-selection replaces the bar with the bulk bar (native
  // parity) — one occupant of the bottom edge, FAB included.
  const { bulkBarPresent } = useMobileChrome()
  const visible = useMobileChromeVisible() && !bulkBarPresent
  const { boardSlug } = useParams({ strict: false })
  // EXP-698: what TAB_BAR_CLEARANCE spends. `0px` on the detail routes that
  // hide the bar (never removed — the literal fallback would reserve a
  // phantom pill there), and 0 on md+ where the element is `display:none`.
  const publishTabBarHeight = useChromeHeightVar(`--tabbar-h`)

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  // chats.json: ONE active team drives every phone screen, the dot too.
  const nav = useThreadsNav(team?.id)
  const openNewDraft = useOpenNewDraft(teamSlug)

  const onBoard = Boolean(
    matchRoute({ to: `/t/$teamSlug/boards/$boardSlug`, fuzzy: true })
  )
  const onTeamIndex = Boolean(matchRoute({ to: `/t/$teamSlug` }))
  const onAgent = Boolean(matchRoute({ to: `/t/$teamSlug/agent`, fuzzy: true }))
  const onChats = Boolean(
    matchRoute({ to: `/t/$teamSlug/threads`, fuzzy: true })
  )
  const onApps = Boolean(matchRoute({ to: `/t/$teamSlug/apps`, fuzzy: true }))
  // The Team tab holds the settings rows and Devices (#EXP-1384: pushed from
  // it now): their pages keep it lit.
  const onTeam =
    Boolean(matchRoute({ to: `/t/$teamSlug/team`, fuzzy: true })) ||
    Boolean(matchRoute({ to: `/t/$teamSlug/settings`, fuzzy: true })) ||
    Boolean(matchRoute({ to: `/t/$teamSlug/devices`, fuzzy: true }))
  // Leaving the board view (or the bar hiding) folds the menu.
  useEffect(() => {
    if (!onBoard || !visible) setMenuOpen(false)
  }, [onBoard, visible])
  // EXP-973: the New-issue arm is always DRAWN; it only goes dead when the
  // team has no board to file into. The arm targets the current board when
  // there is one, else the team's remembered/first board
  // (`resolveBoardTarget`). Same predicate as iOS / Android `composeEnabled`.
  const composeEnabled = boardTarget !== undefined

  if (!visible) return null

  const teamName = team?.name ?? teamSlug

  const menuRows = (boards ?? []).map((board) => ({
    id: board.id,
    name: board.name,
    glyph: <BoardGlyph board={board} className="size-5" />,
    active: board.id === boardTarget?.id,
  }))

  return (
    <>
    <div
      ref={publishTabBarHeight}
      className="pointer-events-none fixed inset-x-0 bottom-0 z-[35] flex items-center justify-center px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:hidden"
    >
      {/* #EXP-1384: the strip (tab capsule + the capsule beside it) is the
          frame the board menu grows from. */}
      <PhoneBoardMenuStrip
        open={menuOpen}
        onOpenChange={setMenuOpen}
        rows={menuRows}
        onSelect={(id) => {
          const board = boards?.find((row) => row.id === id)
          if (board && board.slug !== boardSlug) {
            void navigate({
              to: `/t/$teamSlug/boards/$boardSlug`,
              params: { teamSlug, boardSlug: board.slug },
            })
          }
        }}
        onNewBoard={permissions.canCreate && team ? () => setCreateBoardOpen(true) : undefined}
      >
      <PhoneTabBar className="flex-1">
        {onBoard && boardTarget ? (
          // ON the board view: the caret, and a tap opens the board menu.
          <PhoneTab
            label={boardTarget.name}
            icon={<BoardGlyph board={boardTarget} className="size-5" />}
            trailing={<BoardCaretIcon className="size-2.5 shrink-0" />}
            selected
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            data-testid={CHATS_TEST_IDS.tabBoard}
            onClick={() => setMenuOpen((value) => !value)}
          />
        ) : (
          <PhoneTab
            asChild
            label={boardTarget?.name ?? CHATS_COPY.tabs.board}
            icon={
              boardTarget ? (
                <BoardGlyph board={boardTarget} className="size-5" />
              ) : (
                <BoardTabIcon className="size-5" />
              )
            }
            selected={onBoard || onTeamIndex}
            data-testid={CHATS_TEST_IDS.tabBoard}
          >
            {boardTarget ? (
              <Link
                to="/t/$teamSlug/boards/$boardSlug"
                params={{ teamSlug, boardSlug: boardTarget.slug }}
              />
            ) : (
              <Link to="/t/$teamSlug" params={{ teamSlug }} />
            )}
          </PhoneTab>
        )}
        <PhoneTab
          asChild
          label={CHATS_COPY.tabs.chats}
          icon={<ChatsTabIcon className="size-5" />}
          selected={onChats}
          dot={chatsDot(nav)}
          data-testid="tab-threads"
        >
          <Link to="/t/$teamSlug/threads" params={{ teamSlug }} />
        </PhoneTab>
        <PhoneTab
          asChild
          label={CHATS_COPY.tabs.apps}
          icon={<AppsTabIcon className="size-5" />}
          selected={onApps}
          data-testid="tab-apps"
        >
          <Link to="/t/$teamSlug/apps" params={{ teamSlug }} />
        </PhoneTab>
        <PhoneTab
          asChild
          label={teamName}
          icon={<TeamAvatar name={teamName} size={20} />}
          selected={onTeam}
          data-testid={CHATS_TEST_IDS.tabTeam}
        >
          <Link to="/t/$teamSlug/team" params={{ teamSlug }} />
        </PhoneTab>
      </PhoneTabBar>
      {/* EXP-631/694: the chat launcher started on Devices and Actions,
          EXP-739 made it a LINK to the team's Agent page, EXP-827 merged it
          with New issue on a board (×3 mobile), and EXP-973 made that one
          capsule the whole story — it rides every top-level surface, with the
          New-issue arm dimmed only when the team has no board. EXP-1327: no
          dot here; the live dot rides Threads. */}
      <div className={FAB_GROUP_CLASS} data-testid="fab-group">
        <Link
          to="/t/$teamSlug/agent"
          params={{ teamSlug }}
          aria-label="Start chat"
          data-testid="chat-button"
          aria-current={onAgent ? `page` : undefined}
          className={FAB_ARM_CLASS}
        >
          <span
            className={cn(
              `flex size-11 items-center justify-center rounded-full transition-colors`,
              onAgent && `bg-glass-active`
            )}
          >
            <ActionChatIcon className="size-5" />
          </span>
        </Link>
        <span aria-hidden className="my-3 w-px shrink-0 bg-glass-stroke-card" />
        {composeEnabled ? (
          // EXP-1170: a fresh draft id at TAP time, then the New issue page.
          <Button
            variant="ghost"
            aria-label="New issue"
            data-testid="compose-button"
            className={cn(FAB_ARM_CLASS, `h-auto rounded-none px-0`)}
            onClick={() => openNewDraft({ boardId: boardTarget.id })}
          >
            <NavCreateIssueIcon className="size-5" />
          </Button>
        ) : (
          <span
            role="button"
            aria-disabled
            aria-label="New issue (no board)"
            data-testid="compose-button"
            className={cn(
              FAB_ARM_CLASS,
              `pointer-events-none text-muted-foreground opacity-50`
            )}
          >
            <NavCreateIssueIcon className="size-5" />
          </span>
        )}
      </div>
      </PhoneBoardMenuStrip>
    </div>
    {team && (
      <CreateBoardDialog
        open={createBoardOpen}
        onOpenChange={setCreateBoardOpen}
        team={team}
      />
    )}
    </>
  )
}
