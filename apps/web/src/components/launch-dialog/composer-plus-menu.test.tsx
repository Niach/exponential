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
  subagentModelChoices: () => [],
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
