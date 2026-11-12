// EXP-1635: "Photo or file" on the ONE "+": a phone pushes Camera | Photos |
// File (`addFile.phoneChoices`), a pointer opens the chooser directly.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const phone = vi.hoisted(() => ({ value: false }))

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => phone.value,
}))
vi.mock(`@/components/launch-dialog/issue-picker`, () => ({ ComposerIssuePicker: () => null }))
vi.mock(`@/components/issue-picker-host`, () => ({ IssuePickerBody: () => null }))
vi.mock(`@/components/issue-ref-provider`, () => ({ useIssueRefs: () => null }))
vi.mock(`@/components/launch-dialog/launch-options-pane`, () => ({
  CLI_DEFAULT_EFFORT: `default`,
  effortLabel: (value: string) => value,
  modelChoices: () => [],
}))
vi.mock(`@/components/launch-dialog/mcp-server-picker`, () => ({ McpServerMenuRows: () => null }))
vi.mock(`@/components/launch-dialog/skill-picker`, () => ({ SkillSearchList: () => null }))

const { ComposerOptionsMenu } = await import(`@/components/launch-dialog/composer-plus-menu`)

const openMenu = async (onAttach: (choice?: string) => void) => {
  render(<ComposerOptionsMenu surface="session" agent="claude" onAttach={onAttach} />)
  const plus = screen.getByTestId(`agent-composer-plus-button`)
  // Radix's dropdown opens on pointer down, the sheet's trigger on click.
  fireEvent.pointerDown(plus, { button: 0, ctrlKey: false })
  fireEvent.click(plus)
  return screen.findByTestId(`agent-composer-menu-add-file`)
}

// #FEED-204 (apps.json EDITOR DEVICE): an app editor's Device › on the
// session surface: names the device and stays enabled, even under the ended
// session's `rowsDisabled` (#EXP-1738: a live root's pick moves it).
describe(`an app editor's Device row (#FEED-204)`, () => {
  const devices = [
    { id: `mac`, name: `Mac` },
    { id: `mint`, name: `Mint` },
  ]
  const openDevice = async () => {
    render(
      <ComposerOptionsMenu
        surface="session"
        agent="claude"
        onAttach={() => {}}
        rowsDisabled
        runner={{ device: { value: `mint`, devices, onChange: () => {} } }}
      />
    )
    const plus = screen.getByTestId(`agent-composer-plus-button`)
    fireEvent.pointerDown(plus, { button: 0, ctrlKey: false })
    fireEvent.click(plus)
    return screen.findByTestId(`agent-composer-menu-device`)
  }

  it(`stays enabled and names the device`, async () => {
    phone.value = false
    const row = await openDevice()
    expect(row.textContent).toContain(`Mint`)
    expect(row.hasAttribute(`data-disabled`)).toBe(false)
  })
})

describe(`add-file on the "+" (EXP-1635)`, () => {
  beforeEach(() => {
    phone.value = false
  })

  it(`a pointer opens the chooser directly`, async () => {
    const onAttach = vi.fn()
    fireEvent.click(await openMenu(onAttach))
    expect(onAttach).toHaveBeenCalledExactlyOnceWith()
    expect(screen.queryByTestId(`agent-composer-menu-add-file-camera`)).toBeNull()
  })

  it.each([`camera`, `photos`, `file`])(`a phone pushes the choices: %s`, async (choice) => {
    phone.value = true
    const onAttach = vi.fn()
    fireEvent.click(await openMenu(onAttach))
    expect(onAttach).not.toHaveBeenCalled()
    fireEvent.click(await screen.findByTestId(`agent-composer-menu-add-file-${choice}`))
    expect(onAttach).toHaveBeenCalledExactlyOnceWith(choice)
  })
})

// #EXP-1793 (composer-menu.json `mcpPick`): an agent chat offers MCP
// servers › while the team has a server, its value = the pick's count; a
// session never does.
describe(`MCP servers on an agent chat (#EXP-1793)`, () => {
  const servers = [
    { id: `s1`, name: `GlitchTip` },
    { id: `s2`, name: `Linear` },
  ] as never
  const openOn = async (surface: `agent` | `session`, list = servers) => {
    phone.value = false
    render(
      <ComposerOptionsMenu
        surface={surface}
        agent="claude"
        onAttach={() => {}}
        mcp={{ servers: list, selectedIds: [`s2`], onToggle: () => {}, connectHref: () => null }}
      />
    )
    const plus = screen.getByTestId(`agent-composer-plus-button`)
    fireEvent.pointerDown(plus, { button: 0, ctrlKey: false })
    fireEvent.click(plus)
    await screen.findByTestId(`agent-composer-menu-add-file`)
  }

  it(`an agent chat shows the row with the pick's count`, async () => {
    await openOn(`agent`)
    expect(screen.getByTestId(`agent-composer-menu-mcp-servers`).textContent).toContain(`1`)
  })

  it(`no servers, no row`, async () => {
    await openOn(`agent`, [] as never)
    expect(screen.queryByTestId(`agent-composer-menu-mcp-servers`)).toBeNull()
  })

  it(`a session never shows it`, async () => {
    await openOn(`session`)
    expect(screen.queryByTestId(`agent-composer-menu-mcp-servers`)).toBeNull()
  })
})
