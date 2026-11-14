import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SidebarMenuButton, SidebarMenuItem, SidebarProvider, TooltipProvider } from "@exp/ui"
import fixture from "@exp/domain-contract/fixtures/sidebar-folders.json"
import { sidebarRows, sidebarTree, type SidebarEntry, type SidebarRow } from "@exp/domain-contract/sidebar-folders"

// #EXP-1832 (sidebar-folders.json): one sidebar group as the member arranged
// it. Locks: the rows are `sidebarRows` (a folder row, its items indented
// under it with the tree gutter, a folded folder hides them and shows its
// count), every row carries the genUI List's drag contract
// (`data-reorder-*`), a folder press folds it (never a link), and the
// in-place name field saves on Enter and keeps the default on Esc.

const h = vi.hoisted(() => ({
  rows: [] as SidebarRow[],
  editingId: null as string | null,
  toggle: vi.fn(),
  rename: vi.fn(),
  setEditingId: vi.fn(),
}))

vi.mock(`@/hooks/use-sidebar-folders`, () => ({
  useSidebarFolders: () => ({
    rows: h.rows,
    editingId: h.editingId,
    setEditingId: h.setEditingId,
    move: vi.fn(),
    toggle: h.toggle,
    rename: h.rename,
    ungroup: vi.fn(),
  }),
}))

import { SidebarFolderList } from "@/components/team/sidebar-folders"

const layoutCase = fixture.rows[0]
const rowsFor = (closed: string[]) =>
  sidebarRows(sidebarTree(layoutCase.items, layoutCase.entries as SidebarEntry[]), (id) => !closed.includes(id))

function renderList() {
  return render(
    <TooltipProvider>
      <SidebarProvider>
        <SidebarFolderList
          teamId="t-1"
          section="board"
          ids={layoutCase.items}
          renderItem={(id, itemProps, guides) => (
            <SidebarMenuItem key={id} {...itemProps}>
              {guides}
              <SidebarMenuButton data-testid={`row-${id}`}>{id}</SidebarMenuButton>
            </SidebarMenuItem>
          )}
        />
      </SidebarProvider>
    </TooltipProvider>
  )
}

describe(`#EXP-1832 sidebar folders (web)`, () => {
  beforeEach(() => {
    h.editingId = null
    h.toggle.mockClear()
    h.rename.mockClear()
    h.setEditingId.mockClear()
  })

  it(`renders an open folder with its items indented, every row draggable`, () => {
    h.rows = rowsFor([])
    const { container } = renderList()
    const keys = [...container.querySelectorAll(`[data-reorder-key]`)].map((el) => el.getAttribute(`data-reorder-key`))
    expect(keys).toEqual([`exponential`, `feedback`, `f-mon`, `errors`, `performance`, `next`])
    const folder = screen.getByTestId(`sidebar-folder`)
    expect(folder.getAttribute(`data-reorder-group`)).toBe(`true`)
    expect(folder.textContent).toContain(`Monitoring`)
    const child = screen.getByTestId(`row-errors`).closest(`li`)!
    expect(child.getAttribute(`data-reorder-depth`)).toBe(`1`)
    expect(child.style.paddingLeft).toBe(`14px`)
    // A folder is never a link: its press folds it.
    expect(folder.querySelector(`a`)).toBeNull()
    fireEvent.click(folder.querySelector(`button`)!)
    expect(h.toggle).toHaveBeenCalledWith(`f-mon`)
  })

  it(`a folded folder hides its items and shows its count`, () => {
    h.rows = rowsFor([`f-mon`])
    renderList()
    expect(screen.queryByTestId(`row-errors`)).toBeNull()
    expect(screen.getByTestId(`sidebar-folder`).textContent).toContain(`2`)
  })

  it(`the in-place name: Enter saves, Esc keeps the default`, () => {
    h.rows = rowsFor([])
    h.editingId = `f-mon`
    const { unmount } = renderList()
    const input = screen.getByTestId(`sidebar-folder-name`) as HTMLInputElement
    fireEvent.change(input, { target: { value: `Ops` } })
    fireEvent.keyDown(input, { key: `Enter` })
    expect(h.rename).toHaveBeenCalledWith(`f-mon`, `Ops`)
    unmount()

    renderList()
    fireEvent.keyDown(screen.getByTestId(`sidebar-folder-name`), { key: `Escape` })
    expect(h.setEditingId).toHaveBeenCalledWith(null)
    expect(h.rename).toHaveBeenCalledTimes(1)
  })
})
