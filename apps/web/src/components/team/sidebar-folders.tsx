import { useRef, type HTMLAttributes, type PointerEvent as ReactPointerEvent, type ReactNode } from "react"
import { startListDrag } from "@exponential_app/eui/web"
import { SIDEBAR_FOLDER_DEFAULT_NAME, SIDEBAR_MAX_DEPTH, type SidebarRow, type SidebarSection } from "@exp/domain-contract/sidebar-folders"
import {
  conceptIcon,
  Input,
  Menu,
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
import { useSidebarFolders } from "@/hooks/use-sidebar-folders"
import { cn } from "@/lib/utils"

// #EXP-1832 (fixtures/sidebar-folders.json + the genUI List's `reorderable`
// rules, packages/exponential-ui/fixtures/list-reorder.json): ONE sidebar
// group (Boards, Agents or Apps) as the member arranged it. Rows drag on wide
// web (the shared web controller `startListDrag`, the genUI List's own: the
// line = reorder, the highlighted row = drop INTO); a drop onto a row makes a
// folder whose name is edited in place (Enter saves, Esc keeps "New folder").
// A folder is a row with the fold chevron (+ its count when folded), never a
// link; its right-click menu has Rename and Ungroup. Phones (the sheet) never
// drag.

const FolderIcon = conceptIcon(`ui-folder`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const RenameIcon = conceptIcon(`ui-edit`)
const UngroupIcon = conceptIcon(`ui-folder-open`)

/** The folder menu's kind (`menuProps`), per section. */
export const sidebarFolderMenuKind = (section: SidebarSection) => `sidebar-folder-${section}`

/** The guide gutter starts at the menu button's own left padding. */
const GUIDE_BASE = 8

/** What a row's `SidebarMenuItem` spreads: the drag contract + its indent. */
export type SidebarRowItemProps = HTMLAttributes<HTMLLIElement> & Record<`data-${string}`, string | undefined>

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
  renderItem: (id: string, itemProps: SidebarRowItemProps, guides: ReactNode) => ReactNode
  empty?: ReactNode
  testId?: string
}) {
  const folders = useSidebarFolders(teamId, section, ids)
  const { isMobile } = useSidebar()
  const listRef = useRef<HTMLUListElement>(null)
  const draggable = !isMobile
  const guides = treeGuides(folders.rows.map((row) => row.depth))
  const menuKind = sidebarFolderMenuKind(section)

  const dragProps = (row: SidebarRow): SidebarRowItemProps => ({
    "data-reorder-key": row.id,
    "data-reorder-depth": String(row.depth),
    "data-reorder-group": row.group ? `true` : undefined,
    "data-reorder-indent": String(row.depth * TREE_INDENT),
    "data-sidebar-row": row.kind,
    style: row.depth > 0 ? { paddingLeft: `${row.depth * TREE_INDENT}px` } : undefined,
    onDragStart: (e) => e.preventDefault(),
    onPointerDown: draggable
      ? (e: ReactPointerEvent<HTMLLIElement>) => {
          if ((e.target as Element).closest(`input, textarea, [contenteditable="true"]`)) return
          startListDrag(
            e.nativeEvent,
            row.id,
            {
              rows: () => [...(listRef.current?.querySelectorAll<HTMLElement>(`[data-reorder-key]`) ?? [])],
              maxDepth: SIDEBAR_MAX_DEPTH,
              color: `var(--primary)`,
            },
            { move: (context) => folders.move(context) }
          )
        }
      : undefined,
  })

  const folderMenu = ({ target, open, onOpenChange }: MenuSessionProps) => (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={open}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label="Folder actions"
      title="Folder"
      entries={[
        { kind: `item`, id: `rename`, label: `Rename`, icon: RenameIcon, onSelect: () => folders.setEditingId(target.id) },
        { kind: `item`, id: `ungroup`, label: `Ungroup`, icon: UngroupIcon, onSelect: () => folders.ungroup(target.id) },
      ]}
    />
  )

  return (
    <MenuGestureHost menus={{ [menuKind]: folderMenu }}>
      <SidebarMenu ref={listRef} data-testid={testId}>
        {folders.rows.length === 0 && empty}
        {folders.rows.map((row, index) => {
          const gutter = row.depth > 0 ? <TreeGuides guide={guides[index]} base={GUIDE_BASE} /> : null
          if (row.kind === `item`) return renderItem(row.id, dragProps(row), gutter)
          const editing = folders.editingId === row.id
          return (
            <SidebarMenuItem key={row.id} {...dragProps(row)} data-testid="sidebar-folder">
              <SidebarMenuButton
                density="compact"
                aria-expanded={row.open}
                onClick={() => !editing && folders.toggle(row.id)}
                {...menuProps(menuKind, row.id)}
              >
                <FolderIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
                {editing ? (
                  <FolderNameInput
                    initial={row.name ?? SIDEBAR_FOLDER_DEFAULT_NAME}
                    onSave={(name) => folders.rename(row.id, name)}
                    onCancel={() => folders.setEditingId(null)}
                  />
                ) : (
                  <span className="min-w-0 flex-1 truncate">{row.name}</span>
                )}
              </SidebarMenuButton>
              {!editing && (
                <SidebarMenuTrailing className="pointer-events-none gap-1 text-xs text-muted-foreground">
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

/** The in-place name field: Enter (or leaving it) saves, Esc keeps the name
 *  it had (a new folder's default). */
function FolderNameInput({
  initial,
  onSave,
  onCancel,
}: {
  initial: string
  onSave: (name: string) => void
  onCancel: () => void
}) {
  const done = useRef(false)
  return (
    <Input
      autoFocus
      defaultValue={initial}
      aria-label="Folder name"
      data-testid="sidebar-folder-name"
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
