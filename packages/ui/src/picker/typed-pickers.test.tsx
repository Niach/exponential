import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { Menu } from "../menu"
import { BranchPicker } from "./branch-picker"
import { EffortPicker, ModelPicker } from "./model-picker"
import { PickerMenuRows } from "./picker-menu-rows"
import { RepositoryPicker } from "./repository-picker"

// UI cleanup batch — the typed pickers the composer and the forms moved onto
// (Model, Effort, Repository, Branch) and the menu arm (`PickerMenuRows`):
// one selection language everywhere (single = trailing check, multi = the
// row's highlight), no circle pair.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const rows = () => Array.from(document.querySelectorAll(`[data-slot=command-item]`))

const MODELS = [
  { value: `default`, label: `CLI default` },
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
]

describe(`ModelPicker`, () => {
  it(`is one inline word of the sentence, opening a single pick`, () => {
    const onChange = vi.fn()
    render(<ModelPicker models={MODELS} value="opus" onChange={onChange} />)
    const word = screen.getByRole(`button`, { name: `Model` })
    expect(word.getAttribute(`data-slot`)).toBe(`combobox-inline-trigger`)
    expect(word.textContent).toContain(`Opus`)
    act(() => {
      fireEvent.click(word)
    })
    expect(rows()).toHaveLength(3)
    expect(rows()[1]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    fireEvent.click(rows()[2]!)
    expect(onChange).toHaveBeenCalledWith(`sonnet`)
  })

  it(`collapses to plain text with a single model`, () => {
    render(<ModelPicker models={[MODELS[1]!]} value="opus" onChange={vi.fn()} />)
    expect(screen.queryByRole(`button`)).toBeNull()
    expect(document.querySelector(`[data-slot=combobox-inline-word]`)!.textContent).toBe(`Opus`)
  })
})

describe(`EffortPicker`, () => {
  it(`is a form row: the title leading, the pick trailing`, () => {
    render(
      <EffortPicker
        efforts={[
          { value: `default`, label: `CLI default` },
          { value: `high`, label: `High` },
        ]}
        value="high"
        onChange={vi.fn()}
      />
    )
    const row = document.querySelector(`[data-slot=glass-picker-row]`)!
    expect(row.textContent).toContain(`Effort`)
    expect(row.textContent).toContain(`High`)
  })
})

describe(`RepositoryPicker`, () => {
  it(`draws owner/name rows with the lock, and a None row that clears`, () => {
    const onChange = vi.fn()
    const onNone = vi.fn()
    render(
      <RepositoryPicker
        triggerVariant="row"
        repositories={[
          { id: `r1`, fullName: `acme/app` },
          { id: `r2`, fullName: `acme/secret`, private: true },
        ]}
        value="r1"
        onChange={onChange}
        noneLabel="None"
        onNone={onNone}
      />
    )
    act(() => {
      fireEvent.click(document.querySelector(`[data-slot=glass-picker-row]`)!)
    })
    expect(rows()).toHaveLength(3)
    expect(rows()[2]!.querySelector(`[aria-label=Private]`)).toBeTruthy()
    fireEvent.click(rows()[2]!)
    expect(onChange).toHaveBeenCalledWith(`r2`)
  })
})

describe(`BranchPicker`, () => {
  it(`loads on first open, tags the default and reports it as null`, async () => {
    const loadBranches = vi.fn().mockResolvedValue([`main`, `dev`])
    const onPick = vi.fn()
    render(
      <BranchPicker
        triggerVariant="row"
        label="Branch"
        value="dev"
        defaultBranch="main"
        loadBranches={loadBranches}
        onPick={onPick}
      />
    )
    expect(loadBranches).not.toHaveBeenCalled()
    // Before the list loads the trigger still names the effective branch.
    expect(document.querySelector(`[data-slot=glass-picker-row]`)!.textContent).toContain(`dev`)
    act(() => {
      fireEvent.click(document.querySelector(`[data-slot=glass-picker-row]`)!)
    })
    expect(loadBranches).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(rows()).toHaveLength(2))
    const main = rows().find((row) => row.textContent?.includes(`main`))!
    expect(main.textContent).toContain(`default`)
    fireEvent.click(main)
    expect(onPick).toHaveBeenCalledWith(null)
  })

  it(`keeps a branch the remote no longer has`, async () => {
    render(
      <BranchPicker
        value="gone"
        defaultBranch="main"
        loadBranches={() => Promise.resolve([`main`])}
        onPick={vi.fn()}
      />
    )
    act(() => {
      fireEvent.click(screen.getByRole(`button`))
    })
    await waitFor(() => expect(rows()).toHaveLength(2))
    expect(rows()[0]!.textContent).toContain(`gone`)
  })
})

describe(`PickerMenuRows`, () => {
  const openMenu = () => {
    act(() => {
      fireEvent.pointerDown(screen.getByRole(`button`, { name: `Labels` }), {
        button: 0,
        ctrlKey: false,
        pointerType: `mouse`,
      })
    })
  }

  it(`multi: the row's highlight, no glyph, the menu stays open`, () => {
    const onChange = vi.fn()
    render(
      <Menu
        trigger={<button type="button">Labels</button>}
        body={
          <PickerMenuRows
            mode="multi"
            items={[
              { value: `bug`, label: `Bug`, color: `#ef4444` },
              { value: `ops`, label: `Ops`, checked: `indeterminate` },
              { value: `ui`, label: `UI` },
            ]}
            value={[`bug`]}
            onChange={onChange}
          />
        }
      />
    )
    openMenu()
    const menu = screen.getByRole(`menu`)
    const boxes = within(menu).getAllByRole(`menuitemcheckbox`)
    expect(boxes.map((box) => box.getAttribute(`data-picked`))).toEqual([`true`, `mixed`, null])
    expect(boxes[1]!.getAttribute(`aria-checked`)).toBe(`mixed`)
    expect(menu.querySelectorAll(`[data-selected-glyph]`)).toHaveLength(0)
    fireEvent.click(boxes[2]!)
    expect(onChange).toHaveBeenCalledWith([`bug`, `ui`])
    expect(screen.getByRole(`menu`)).toBeTruthy()
  })

  it(`single: a none row reports through onNone; indeterminate marks nothing`, () => {
    const onNone = vi.fn()
    render(
      <Menu
        trigger={<button type="button">Labels</button>}
        body={
          <PickerMenuRows
            mode="single"
            items={[{ value: `a`, label: `Alice` }]}
            value={null}
            indeterminate
            noneLabel="Unassigned"
            onNone={onNone}
            onChange={vi.fn()}
          />
        }
      />
    )
    openMenu()
    const menu = screen.getByRole(`menu`)
    expect(menu.querySelectorAll(`[data-selected-glyph]`)).toHaveLength(0)
    fireEvent.click(within(menu).getByText(`Unassigned`))
    expect(onNone).toHaveBeenCalledTimes(1)
  })
})
