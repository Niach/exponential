import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import spec from "@exp/domain-contract/fixtures/picker-presentation.json"
import { ComposerOptionsRow } from "../composer-frame"
import { Dialog, DialogContent, DialogTitle } from "../dialog"
import { Picker, type PickerItem } from "./picker"
import { PICKER_INLINE_MAX_HEIGHT, pickerPlacement, type PickerPlacementInput } from "./picker-placement"

// N-14 — picker-presentation.json: WHERE a picker's rows open. The
// `placementCases` replay the pure rule; the row cases pin the composer
// options row in a dialog (THE start-session dialog's pins).

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

describe(`pickerPlacement (picker-presentation.json placementCases)`, () => {
  for (const testCase of spec.placementCases) {
    it(testCase.name, () => {
      expect(pickerPlacement(testCase.input as PickerPlacementInput)).toEqual(testCase.expect)
    })
  }

  it(`caps the inline rows at inlineMaxHeight`, () => {
    expect(PICKER_INLINE_MAX_HEIGHT).toBe(spec.inlineMaxHeight)
  })
})

const models: PickerItem[] = [
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
]
const devices: PickerItem[] = [
  { value: `mac`, label: `Mac` },
  { value: `server`, label: `Server` },
]

function OptionsRowInDialog({ onModel = vi.fn() }: { onModel?: (value: string) => void }) {
  return (
    <Dialog open>
      <DialogContent>
        <DialogTitle>Start session</DialogTitle>
        <ComposerOptionsRow data-testid="row">
          <Picker
            mode="single"
            items={devices}
            value="mac"
            onChange={vi.fn()}
            mobileTitle="Device"
            triggerVariant="inline"
          />
          <Picker
            mode="single"
            items={models}
            value="opus"
            onChange={onModel}
            mobileTitle="Model"
            triggerVariant="inline"
          />
        </ComposerOptionsRow>
      </DialogContent>
    </Dialog>
  )
}

const trigger = (name: string) => screen.getByRole(`button`, { name })
const click = (element: Element) => {
  act(() => {
    fireEvent.click(element)
  })
}
const slot = () => document.querySelector(`[data-slot=inline-picker-slot]`)!
const bodies = () => Array.from(document.querySelectorAll(`[data-slot=combobox-inline-body]`))

describe(`the composer options row in a dialog (N-14)`, () => {
  it(`collapsed, every pin is its bare trigger: no stretching column around it`, () => {
    render(<OptionsRowInDialog />)
    const row = screen.getByTestId(`row`)
    for (const wrapper of row.querySelectorAll(`[data-slot=combobox-inline]`)) {
      expect(wrapper.className).toBe(`contents`)
      expect(wrapper.getAttribute(`data-state`)).toBe(`closed`)
    }
    expect(row.querySelectorAll(`[data-slot=combobox-inline]`)).toHaveLength(2)
    for (const pin of row.querySelectorAll(`[data-slot=combobox-inline-trigger]`)) {
      // Nothing between the pin and the row may be a box of its own.
      for (let parent = pin.parentElement; parent && parent !== row; parent = parent.parentElement) {
        expect(parent.className).toBe(`contents`)
      }
    }
    expect(document.querySelector(`[data-picker-body=rowSlot]`)).not.toBeNull()
    expect(bodies()).toHaveLength(0)
    expect(trigger(`Model`).getAttribute(`aria-expanded`)).toBe(`false`)
  })

  it(`opening a pin renders its rows in the slot UNDER the row, as wide as the row`, () => {
    render(<OptionsRowInDialog />)
    const row = screen.getByTestId(`row`)
    click(trigger(`Model`))
    expect(trigger(`Model`).getAttribute(`aria-expanded`)).toBe(`true`)
    const [body] = bodies()
    expect(body).toBeDefined()
    expect(slot().contains(body!)).toBe(true)
    expect(row.contains(body!)).toBe(false)
    // The slot is the row's next sibling: under the whole row.
    expect(row.nextElementSibling).toBe(slot())
    expect(body!.className).toContain(`max-h-72`)
    expect(body!.className).toContain(`w-full`)
    expect(body!.textContent).toContain(`Sonnet`)
  })

  it(`opening a second pin replaces the first one's rows`, () => {
    render(<OptionsRowInDialog />)
    click(trigger(`Model`))
    click(trigger(`Device`))
    expect(bodies()).toHaveLength(1)
    expect(bodies()[0]!.textContent).toContain(`Server`)
    expect(trigger(`Model`).getAttribute(`aria-expanded`)).toBe(`false`)
    expect(trigger(`Device`).getAttribute(`aria-expanded`)).toBe(`true`)
  })

  it(`a single pick folds the rows and the row is as before`, () => {
    const onModel = vi.fn()
    render(<OptionsRowInDialog onModel={onModel} />)
    click(trigger(`Model`))
    click(screen.getByText(`Sonnet`))
    expect(onModel).toHaveBeenCalledWith(`sonnet`)
    expect(bodies()).toHaveLength(0)
    expect(slot().childElementCount).toBe(0)
  })

  it(`the trigger again, Escape and a click outside fold the rows; the dialog stays`, () => {
    render(<OptionsRowInDialog />)
    click(trigger(`Model`))
    click(trigger(`Model`))
    expect(bodies()).toHaveLength(0)

    click(trigger(`Model`))
    act(() => {
      fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })
    })
    expect(bodies()).toHaveLength(0)
    expect(screen.getByText(`Start session`)).not.toBeNull()

    click(trigger(`Model`))
    act(() => {
      fireEvent.click(screen.getByText(`Start session`))
    })
    expect(bodies()).toHaveLength(0)
  })

  it(`outside a row a dialog picker discloses directly under its trigger`, () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Form</DialogTitle>
          <Picker mode="single" items={models} value={null} onChange={vi.fn()} mobileTitle="Model" triggerVariant="field" />
        </DialogContent>
      </Dialog>
    )
    expect(document.querySelector(`[data-picker-body=underTrigger]`)).not.toBeNull()
    click(screen.getByRole(`button`, { name: /Model/ }))
    const [body] = bodies()
    expect(body!.getAttribute(`data-inline-body`)).toBe(`underTrigger`)
    expect(document.querySelector(`[data-slot=inline-picker-slot]`)).toBeNull()
  })
})
