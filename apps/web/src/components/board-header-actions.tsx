import { useNavigate } from "@tanstack/react-router"
import { SIDEBAR_MENU_COPY } from "@exp/domain-contract/sidebar-menus"
import { Button, conceptIcon, HEADER_ACTION_CLASS, HeaderAction, Menu } from "@exp/ui"
import type { Board } from "@/db/schema"
import { useBoardArchive } from "@/components/team/board-archive"
import { headerMenuEntries, openHandlers, sidebarMenuEntries } from "@/components/team/sidebar-menus"

// header-actions.json `board`: + = New issue on it, `…` = the board's menu
// (Copy link; owners: Settings, Archive).

const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)

export function BoardHeaderActions({
  board,
  teamSlug,
  owner,
  onNewIssue,
}: {
  board: Board
  teamSlug: string
  owner: boolean
  onNewIssue: () => void
}) {
  const navigate = useNavigate()
  const archive = useBoardArchive(() => true)
  const base = `/t/${encodeURIComponent(teamSlug)}`
  const entries = headerMenuEntries(
    sidebarMenuEntries(`board`, { owner }, {
      ...openHandlers(`${base}/boards/${encodeURIComponent(board.slug)}`, () => undefined, null),
      settings: () => void navigate({ href: `${base}/settings/boards/${board.id}` }),
      archive: () => archive.ask(board),
    })
  )
  return (
    <span className="flex shrink-0 items-center gap-2">
      <HeaderAction
        aria-label={SIDEBAR_MENU_COPY[`new-issue`]}
        title={SIDEBAR_MENU_COPY[`new-issue`]}
        data-testid="board-new-issue"
        onClick={onNewIssue}
      >
        <AddIcon />
      </HeaderAction>
      <Menu
        align="end"
        aria-label={`${board.name} actions`}
        title={board.name}
        entries={entries}
        trigger={
          <Button variant="ghost" size="icon" className={HEADER_ACTION_CLASS} aria-label={`${board.name} actions`} data-testid="board-menu">
            <MoreIcon />
          </Button>
        }
      />
      {archive.dialog}
    </span>
  )
}
