import { createFileRoute, Link, Outlet, redirect } from "@tanstack/react-router"
import { Fragment, useEffect, useRef, useState } from "react"
import { Separator, SEGMENTED_ITEM, SEGMENTED_LIST, conceptIcon } from "@exp/ui"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { cn } from "@/lib/utils"
import { useTeamBoards } from "@/hooks/use-team-data"
import { BoardGlyph } from "@/components/board-glyph"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import {
  NEW_BOARD_LABEL,
  SETTINGS_BOARDS_GROUP,
  SETTINGS_NAV,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

const AddIcon = conceptIcon(`ui-add`)

export const Route = createFileRoute(`/t/$teamSlug/settings`)({
  head: () => ({ meta: [{ title: pageTitle(`Settings`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SettingsLayout,
})

function SettingsLayout() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, config } = useSettingsPage(teamSlug)
  const navContext = { isCloud: Boolean(config?.isCloud) }
  const navItems = SETTINGS_NAV.flatMap((group) =>
    group.items.filter((item) => item.visible(permissions, navContext))
  )
  // EXP-862: the Boards group lists every board, here as well as in the
  // sidebar panel — this strip is the ONLY settings nav on a phone.
  const boards = useTeamBoards(team?.id)
  const showBoards = permissions.isOwner
  const [createOpen, setCreateOpen] = useState(false)

  // EXP-698: the strip is wider than a phone, so the section you are ON can
  // start scrolled out of frame ("…torage"). Bring it into view once the row
  // is populated — permissions land async, so this keys on the item count
  // rather than plain mount. `nearest` on both axes means an already-visible
  // tab is left alone and the page never scrolls vertically. EXP-862: the
  // strip also lists the boards (they hydrate off Electric, separately from
  // permissions), so a board page keys on that count too — or its own row
  // could still be off-screen when the nav items settled first.
  const navRef = useRef<HTMLElement | null>(null)
  const boardRows = showBoards ? boards.length : 0
  useEffect(() => {
    navRef.current
      ?.querySelector(`[aria-current="page"]`)
      ?.scrollIntoView({ inline: `nearest`, block: `nearest` })
  }, [navItems.length, boardRows])

  return (
    <div
      className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${TAB_BAR_CLEARANCE}`}
    >
      <div>
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage {team?.name ?? `your team`} and your account
        </p>
      </div>

      <Separator />

      <div className="flex flex-col gap-6">
        {/* EXP-456: on md+ the settings nav lives in the app sidebar (the
            main nav slides out, the settings nav slides in — see
            TeamSidebar/SettingsSidebar), so this in-page nav is the mobile
            horizontally-scrollable row only (group labels hidden there). */}
        {/* EXP-616: iOS capsule segmented control look — these are route
            links, not stateful tabs, so they borrow the segmented-control
            classes (EXP-698) rather than forcing Radix Tabs semantics onto
            them. Groups flatten into one strip (their labels were already
            hidden here); the strip still scrolls horizontally, with the
            scrollbar itself hidden so the capsule edge stays clean.
            EXP-698: `px-1` keeps the first and last pill off the capsule's
            rounded edge — flush against it they read as clipped. */}
        <nav
          ref={navRef}
          className={cn(
            SEGMENTED_LIST,
            `max-w-full gap-1 self-start overflow-x-auto px-1 scrollbar-none md:hidden`
          )}
        >
          {SETTINGS_NAV.map((group) => (
            <Fragment key={group.group}>
              {group.group === SETTINGS_BOARDS_GROUP && showBoards && (
                <>
                  {boards.map((board) => (
                    <Link
                      key={board.id}
                      to="/t/$teamSlug/settings/boards/$boardId"
                      params={{ teamSlug, boardId: board.id }}
                      className={SEGMENTED_ITEM}
                      activeProps={{
                        className: `border-glass-stroke-active bg-glass-active text-foreground`,
                      }}
                      inactiveProps={{
                        className: `border-transparent text-muted-foreground hover:text-foreground`,
                      }}
                    >
                      <BoardGlyph board={board} />
                      {board.name}
                    </Link>
                  ))}
                  {team && (
                    <button
                      type="button"
                      onClick={() => setCreateOpen(true)}
                      className={cn(
                        SEGMENTED_ITEM,
                        `border-transparent text-muted-foreground hover:text-foreground`
                      )}
                    >
                      <AddIcon className="h-4 w-4" />
                      {NEW_BOARD_LABEL}
                    </button>
                  )}
                </>
              )}
              {group.items
                .filter((item) => item.visible(permissions, navContext))
                .map((item) => (
                  <Link
                    key={item.label}
                    to={item.to}
                    params={{ teamSlug }}
                    className={SEGMENTED_ITEM}
                    activeProps={{
                      className: `border-glass-stroke-active bg-glass-active text-foreground`,
                    }}
                    inactiveProps={{
                      className: `border-transparent text-muted-foreground hover:text-foreground`,
                    }}
                  >
                    <item.icon className="h-4 w-4" />
                    {item.label}
                  </Link>
                ))}
            </Fragment>
          ))}
        </nav>

        <div className="min-w-0 flex-1">
          <Outlet />
        </div>
      </div>

      {team && (
        <CreateBoardDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          team={team}
        />
      )}
    </div>
  )
}
