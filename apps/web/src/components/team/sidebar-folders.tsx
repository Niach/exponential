import { createContext, Fragment, useContext, useRef, type HTMLAttributes, type PointerEvent as ReactPointerEvent, type ReactNode } from "react"
import { startListDrag } from "@exponential_app/eui/web"
import { SIDEBAR_FOLDER_DEFAULT_NAME, SIDEBAR_MAX_DEPTH, type SidebarSection } from "@exp/domain-contract/sidebar-folders"
import { SIDEBAR_TREE_MAX_DEPTH, sidebarTeamRows, type SidebarTreeRow } from "@exp/domain-contract/sidebar-tree"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"
import {
  Badge,
  conceptIcon,
  Input,
  MenuGestureHost,
  menuProps,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuTrailing,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
  useSidebar,
  type MenuSessionProps,
} from "@exp/ui"
import { useSidebarFolders, type SidebarFolders } from "@/hooks/use-sidebar-folders"
import { cn } from "@/lib/utils"
import { sidebarMenuEntries, SidebarPointerMenu } from "@/components/team/sidebar-menus"

// #EXP-1832 (fixtures/sidebar-folders.json + the genUI List's `reorderable`
// rules, packages/exponential-ui/fixtures/list-reorder.json): ONE sidebar
// list as the member arranged it (#EXP-2027: a team's one mixed list, under
// its team row). Rows drag on wide
// web (the shared web controller `startListDrag`, the genUI List's own: the
// line = reorder, the highlighted row = drop INTO); a drop onto a row makes a
// folder whose name is edited in place (Enter saves, Esc keeps "New folder").
// A folder is a row with the fold chevron (+ its count when folded), never a
// link; its right-click menu is THE table's `folder` row (#EXP-2124,
// sidebar-menus.json): Rename and Ungroup. Phones (the sheet) never
// drag.

/** #EXP-2109: the phones' Teams tab draws the SAME rows at touch density
 *  (`SidebarMenuButton density="touch"`); the sidebar is `compact`. */
export type SidebarTreeDensity = `compact` | `touch`
export const SidebarTreeDensityContext = createContext<SidebarTreeDensity>(`compact`)
export const useSidebarTreeDensity = () => useContext(SidebarTreeDensityContext)

/** Where the guide gutters start: the row's own leading padding. */
const GUIDE_BASES: Record<SidebarTreeDensity, number> = { compact: 8, touch: 16 }

const FolderIcon = conceptIcon(`ui-folder`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)

/** The folder menu's kind (`menuProps`), per section (and per team: every
 *  team's List hosts its own folder menu, #EXP-2027). */
export const sidebarFolderMenuKind = (section: SidebarSection, teamId?: string) =>
  teamId ? `sidebar-folder-${section}:${teamId}` : `sidebar-folder-${section}`

/** What a row's `SidebarMenuItem` spreads: the drag contract + its indent. */
export type SidebarRowItemProps = HTMLAttributes<HTMLLIElement> & Record<`data-${string}`, string | undefined>

type RenderItem = (id: string, itemProps: SidebarRowItemProps, guides: ReactNode) => ReactNode

export function SidebarFolderList({
  teamId,
  section,
  ids,
  renderItem,
  empty,
  testId,
}: {
  teamId: string | undefined
  section: SidebarSection
  /** The section's visible targets in DEFAULT order. */
  ids: readonly string[]
  /** One target's row: a `SidebarMenuItem` spreading `itemProps`, with the
   *  `guides` (the tree gutter of a row inside a folder) as its first child. */
  renderItem: RenderItem
  empty?: ReactNode
  testId?: string
}) {
  const folders = useSidebarFolders(teamId, section, ids)
  return (
    <SidebarFolderRows
      folders={folders}
      teamId={teamId}
      section={section}
      renderItem={renderItem}
      empty={empty}
      testId={testId}
    />
  )
}

/** #EXP-2027 (sidebar-tree.json): a TEAM's list: its row heads the List at
 *  depth 0 (`sidebarTeamRows`), its nodes one level deeper, maxDepth 3. */
export interface SidebarFolderTeamHead {
  open: boolean
  /** The team row: a `SidebarMenuItem` spreading `itemProps`. */
  render: (itemProps: SidebarRowItemProps) => ReactNode
}

/** The rows of one list over a `useSidebarFolders` the caller owns. */
export function SidebarFolderRows({
  folders,
  teamId,
  section,
  renderItem,
  empty,
  testId,
  team,
  menus,
  listProps,
  folderBadge,
}: {
  folders: SidebarFolders
  teamId: string | undefined
  section: SidebarSection
  renderItem: RenderItem
  empty?: ReactNode
  testId?: string
  /** Heads the List with the team row (section `team`). */
  team?: SidebarFolderTeamHead
  /** More context menus this List's host answers (the team's own). */
  menus?: Record<string, (session: MenuSessionProps) => ReactNode>
  listProps?: Record<`data-${string}`, string>
  /** nav-badges.json: a folded folder's count badge over its hidden items. */
  folderBadge?: (itemIds: readonly string[]) => CountBadgeValue
}) {
  const { isMobile } = useSidebar()
  const listRef = useRef<HTMLUListElement>(null)
  const density = useSidebarTreeDensity()
  const draggable = !isMobile && density === `compact`
  const menuKind = sidebarFolderMenuKind(section, team ? teamId : undefined)
  const rows: SidebarTreeRow[] =
    team && teamId ? sidebarTeamRows({ teamId, open: team.open, rows: folders.rows }) : folders.rows
  const maxDepth = team ? SIDEBAR_TREE_MAX_DEPTH : SIDEBAR_MAX_DEPTH
  const guides = treeGuides(rows.map((row) => row.depth))

  const dragProps = (row: SidebarTreeRow): SidebarRowItemProps => {
    const indent = row.depth * TREE_INDENT
    return {
      "data-reorder-key": row.id,
      "data-reorder-depth": String(row.depth),
      "data-reorder-group": row.group ? `true` : undefined,
      "data-reorder-indent": String(indent),
      "data-sidebar-row": row.kind,
      style: row.depth > 0 ? { paddingLeft: `${indent}px` } : undefined,
      onDragStart: (e) => e.preventDefault(),
      onPointerDown:
        draggable && row.kind !== `team`
          ? (e: ReactPointerEvent<HTMLLIElement>) => {
              if ((e.target as Element).closest(`input, textarea, [contenteditable="true"]`)) return
              startListDrag(
                e.nativeEvent,
                row.id,
                {
                  rows: () => [...(listRef.current?.querySelectorAll<HTMLElement>(`[data-reorder-key]`) ?? [])],
                  maxDepth,
                  color: `var(--primary)`,
                },
                { move: (context) => folders.move(context) }
              )
            }
          : undefined,
    }
  }

  // #EXP-2124: THE table's `folder` menu (Rename, Ungroup).
  const folderMenu = (session: MenuSessionProps) => (
    <SidebarPointerMenu
      session={session}
      title={folders.rows.find((row) => row.id === session.target.id)?.name ?? SIDEBAR_FOLDER_DEFAULT_NAME}
      entries={sidebarMenuEntries(`folder`, {}, {
        rename: () => folders.setEditingId(session.target.id),
        ungroup: () => folders.ungroup(session.target.id),
      })}
    />
  )

  return (
    <MenuGestureHost menus={{ ...menus, [menuKind]: folderMenu }}>
      <SidebarMenu ref={listRef} className={density === `touch` ? `gap-0` : undefined} data-testid={testId} {...listProps}>
        {folders.rows.length === 0 && !team && empty}
        {rows.map((row, index) => {
          const gutter = row.depth > 0 ? <TreeGuides guide={guides[index]} base={GUIDE_BASES[density]} /> : null
          if (row.kind === `team`) return <Fragment key={row.id}>{team?.render(dragProps(row))}</Fragment>
          if (row.kind === `item`) return renderItem(row.id, dragProps(row), gutter)
          const editing = folders.editingId === row.id
          const badge = row.open ? null : folderBadge?.(folders.itemsOf(row.id))
          return (
            <SidebarMenuItem key={row.id} {...dragProps(row)} data-testid="sidebar-folder">
              {gutter}
              <SidebarMenuButton
                density={density}
                aria-expanded={row.open}
                onClick={() => !editing && folders.toggle(row.id)}
                {...menuProps(menuKind, row.id)}
              >
                <FolderIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
                {editing ? (
                  <SidebarNameInput
                    label="Folder name"
                    testId="sidebar-folder-name"
                    initial={row.name ?? SIDEBAR_FOLDER_DEFAULT_NAME}
                    onSave={(name) => folders.rename(row.id, name)}
                    onCancel={() => folders.setEditingId(null)}
                  />
                ) : (
                  <span className="min-w-0 flex-1 truncate">{row.name}</span>
                )}
              </SidebarMenuButton>
              {!editing && (
                <SidebarMenuTrailing className={cn(`pointer-events-none gap-1 text-xs text-muted-foreground`, density === `touch` && `right-4 justify-end`)}>
                  {badge && <Badge count={badge.count} tone={badge.tone} data-testid="sidebar-folder-badge" />}
                  {!row.open && <span className="tabular-nums">{row.count}</span>}
                  <ChevronIcon className={cn(`size-4 transition-transform`, row.open && `rotate-90`)} />
                </SidebarMenuTrailing>
              )}
            </SidebarMenuItem>
          )
        })}
      </SidebarMenu>
    </MenuGestureHost>
  )
}

/** The in-place name field (a folder's; #EXP-2124 a device's Rename too):
 *  Enter (or leaving it) saves, Esc keeps the name it had (a new folder's
 *  default). */
export function SidebarNameInput({
  label,
  testId,
  initial,
  onSave,
  onCancel,
}: {
  label: string
  testId: string
  initial: string
  onSave: (name: string) => void
  onCancel: () => void
}) {
  const done = useRef(false)
  return (
    <Input
      autoFocus
      defaultValue={initial}
      aria-label={label}
      data-testid={testId}
      className="h-6 min-w-0 flex-1 px-1.5 py-0 text-sm"
      onFocus={(e) => e.currentTarget.select()}
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation()
        if (e.key === `Enter`) {
          e.preventDefault()
          done.current = true
          onSave(e.currentTarget.value)
        } else if (e.key === `Escape`) {
          e.preventDefault()
          done.current = true
          onCancel()
        }
      }}
      onBlur={(e) => {
        if (done.current) return
        done.current = true
        onSave(e.currentTarget.value)
      }}
    />
  )
}
