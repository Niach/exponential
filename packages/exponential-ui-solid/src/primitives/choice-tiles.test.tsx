import { fireEvent, render, screen } from "@solidjs/testing-library"
import { describe, expect, it, vi } from "vitest"

import { ChoiceTiles, choiceColumns, choiceToggle, choiceValues } from "./choice-tiles"

// EXP-1280: THE picture choice (React's choice-tiles.test.tsx).

const OPTIONS = [
  { value: `two`, label: `Two columns`, description: `Pictures beside the text`, image: `/api/attachments/a1` },
  { value: `one`, label: `One column`, image: null },
  { value: `off`, label: `Off`, disabled: true },
  { value: `grid`, label: `Grid`, image: `/api/attachments/a3` },
]

describe(`ChoiceTiles`, () => {
  it(`is a radiogroup of radio tiles; a press picks one value`, () => {
    const onChange = vi.fn()
    render(() => <ChoiceTiles options={OPTIONS} value="two" onChange={onChange} data-testid="tiles" />)
    const group = screen.getByTestId(`tiles`)
    expect(group.getAttribute(`role`)).toBe(`radiogroup`)
    expect(group.style.gridTemplateColumns).toBe(`repeat(2, minmax(0, 1fr))`)
    const radios = screen.getAllByRole(`radio`)
    expect(radios.map((r) => r.getAttribute(`aria-checked`))).toEqual([`true`, `false`, `false`, `false`])
    expect(radios[0]!.className).toContain(`border-primary`)
    fireEvent.click(radios[3]!)
    expect(onChange).toHaveBeenLastCalledWith(`grid`)
    fireEvent.click(radios[2]!)
    expect(onChange).toHaveBeenCalledTimes(1)
  })

  it(`multiple = checkbox tiles toggling a list in option order`, () => {
    const onChange = vi.fn()
    render(() => <ChoiceTiles options={OPTIONS} value={[`grid`]} multiple onChange={onChange} />)
    const boxes = screen.getAllByRole(`checkbox`)
    fireEvent.click(boxes[0]!)
    expect(onChange).toHaveBeenLastCalledWith([`two`, `grid`])
    fireEvent.click(boxes[3]!)
    expect(onChange).toHaveBeenLastCalledWith([])
  })

  it(`keeps one picture box per tile once any option has a picture; renderImage draws it`, () => {
    const renderImage = vi.fn((src: string, alt: string) => <img data-testid={`img-${src}`} src={src} alt={alt} />)
    const { container } = render(() => <ChoiceTiles options={OPTIONS} value="" renderImage={renderImage} />)
    expect(container.querySelectorAll(`[data-slot="choice-tile-picture"]`)).toHaveLength(4)
    expect(renderImage).toHaveBeenCalledWith(`/api/attachments/a1`, `Two columns`)
    expect(screen.getByTestId(`img-/api/attachments/a3`)).toBeTruthy()
    // A tile without a picture shows the placeholder glyph in the same box.
    expect(container.querySelectorAll(`[data-slot="choice-tile-picture"] svg`)).toHaveLength(2)
    // Text-only options: no picture boxes at all.
    const { container: plain } = render(() => <ChoiceTiles options={[{ value: `a`, label: `A` }]} value="a" columns={9} />)
    expect(plain.querySelectorAll(`[data-slot="choice-tile-picture"]`)).toHaveLength(0)
    expect((plain.firstChild as HTMLElement).style.gridTemplateColumns).toBe(`repeat(4, minmax(0, 1fr))`)
  })

  it(`roving tab stop: arrows move with wrap, skip disabled tiles, a single pick follows focus`, () => {
    const onChange = vi.fn()
    render(() => <ChoiceTiles options={OPTIONS} value="one" onChange={onChange} />)
    const radios = screen.getAllByRole(`radio`)
    expect(radios.map((r) => r.tabIndex)).toEqual([-1, 0, -1, -1])
    radios[1]!.focus()
    fireEvent.keyDown(radios[1]!, { key: `ArrowRight` })
    expect(document.activeElement).toBe(radios[3])
    expect(onChange).toHaveBeenLastCalledWith(`grid`)
    fireEvent.keyDown(radios[3]!, { key: `ArrowDown` })
    expect(document.activeElement).toBe(radios[0])
    fireEvent.keyDown(radios[0]!, { key: `ArrowUp` })
    expect(document.activeElement).toBe(radios[3])
  })

  it(`disabled = every tile inert`, () => {
    const onChange = vi.fn()
    render(() => <ChoiceTiles options={OPTIONS} value="two" disabled onChange={onChange} />)
    for (const r of screen.getAllByRole(`radio`)) fireEvent.click(r)
    expect(onChange).not.toHaveBeenCalled()
  })

  it(`the model helpers match fixtures/interactions-round1.json`, () => {
    expect(choiceValues(`a, c,`, true)).toEqual([`a`, `c`])
    expect(choiceValues(`a,c`, false)).toEqual([`a,c`])
    expect(choiceValues(null, false)).toEqual([])
    expect(choiceToggle([`c`], `a`, true, [`a`, `b`, `c`])).toEqual([`a`, `c`])
    expect(choiceToggle([`a`, `b`], `a`, true, [`a`, `b`, `c`])).toEqual([`b`])
    expect(choiceToggle([`a`], `c`, false, [`a`, `b`, `c`])).toEqual([`c`])
    expect([choiceColumns(undefined), choiceColumns(0), choiceColumns(3.4), choiceColumns(12)]).toEqual([2, 1, 3, 4])
  })
})
