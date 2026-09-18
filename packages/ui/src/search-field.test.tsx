import * as React from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { SearchField } from "./search-field"

// EXP-941: the web twin of the natives' `GlassSheetSearchField` — the glyph
// and the clear button are part of the FIELD, not something eight call sites
// each remember to add.

const field = () => screen.getByRole(`searchbox`)

describe(`SearchField`, () => {
  it(`carries the search glyph and no clear button while empty`, () => {
    const { container } = render(
      <SearchField value="" onValueChange={vi.fn()} placeholder="Filter files" />
    )
    expect(container.querySelector(`svg[aria-hidden]`)).toBeTruthy()
    expect(screen.queryByLabelText(`Clear search`)).toBeNull()
    expect(field().getAttribute(`type`)).toBe(`search`)
    // The native WebKit decoration would be a SECOND clear button.
    expect(field().className).toContain(
      `[&::-webkit-search-cancel-button]:hidden`
    )
  })

  it(`reports every keystroke`, () => {
    const onValueChange = vi.fn()
    render(<SearchField value="" onValueChange={onValueChange} />)
    fireEvent.change(field(), { target: { value: `read` } })
    expect(onValueChange).toHaveBeenCalledWith(`read`)
  })

  it(`clears to the empty string and puts the caret back`, () => {
    const onValueChange = vi.fn()
    render(<SearchField value="read" onValueChange={onValueChange} />)
    const clear = screen.getByLabelText(`Clear search`)
    fireEvent.click(clear)
    expect(onValueChange).toHaveBeenCalledWith(``)
    expect(document.activeElement).toBe(field())
  })

  it(`names the clear button whatever the host calls it`, () => {
    render(
      <SearchField
        value="x"
        onValueChange={vi.fn()}
        clearLabel="Clear emoji search"
      />
    )
    expect(screen.getByLabelText(`Clear emoji search`)).toBeTruthy()
  })

  it(`drops the clear button when clearable is false`, () => {
    render(<SearchField value="x" onValueChange={vi.fn()} clearable={false} />)
    expect(screen.queryByLabelText(`Clear search`)).toBeNull()
  })

  it(`has two rungs: the stock field and the dense one`, () => {
    const { rerender } = render(<SearchField value="" onValueChange={vi.fn()} />)
    expect(field().className).toContain(`h-9`)
    rerender(<SearchField value="" onValueChange={vi.fn()} size="sm" />)
    expect(field().className).toContain(`h-7`)
    expect(field().className).toContain(`text-xs`)
  })

  it(`forwards the ref, the key handler, the label and the testid`, () => {
    const ref = React.createRef<HTMLInputElement>()
    const onKeyDown = vi.fn()
    render(
      <SearchField
        ref={ref}
        value=""
        onValueChange={vi.fn()}
        onKeyDown={onKeyDown}
        aria-label="Filter files"
        data-testid="diff-nav-filter"
      />
    )
    expect(ref.current).toBe(screen.getByTestId(`diff-nav-filter`))
    expect(screen.getByLabelText(`Filter files`)).toBe(ref.current)
    fireEvent.keyDown(ref.current!, { key: `Enter` })
    expect(onKeyDown).toHaveBeenCalled()
  })
})
