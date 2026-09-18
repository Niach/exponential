import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Composer, ComposerSubmit, ComposerTool } from "./composer"

// EXP-698/EXP-961: the ONE composer card — comments, steering, support
// replies and the chat launcher all render this chrome.

const card = (container: HTMLElement) =>
  container.querySelector(`[data-slot="composer"]`)!

describe(`Composer`, () => {
  it(`stacks the leading row, the strip, the field and the tool row`, () => {
    const { container } = render(
      <Composer
        leading={<span>Reply</span>}
        strip={<span>one.png</span>}
        tools={<ComposerTool aria-label="Attach" />}
        submit={<ComposerSubmit />}
      >
        <textarea aria-label="Message" />
      </Composer>
    )
    expect(screen.getByText(`Reply`)).toBeTruthy()
    expect(screen.getByText(`one.png`)).toBeTruthy()
    expect(screen.getByLabelText(`Message`)).toBeTruthy()
    expect(screen.getByRole(`button`, { name: `Attach` })).toBeTruthy()
    // The tool row is the field's SIBLING, not part of its row.
    const field = screen.getByLabelText(`Message`)
    const tools = screen.getByRole(`button`, { name: `Attach` })
    expect(tools.closest(`[data-slot="composer"]`)).toBe(card(container))
    expect(field.parentElement).toBe(card(container))
  })

  it(`puts the tools and the submit on the field's own row when inline`, () => {
    render(
      <Composer
        inline
        tools={<ComposerTool aria-label="Attach" />}
        submit={<ComposerSubmit />}
      >
        <textarea aria-label="Message" />
      </Composer>
    )
    const row = screen.getByLabelText(`Message`).parentElement!.parentElement!
    expect(row.className).toContain(`items-end`)
    expect(row.contains(screen.getByRole(`button`, { name: `Attach` }))).toBe(
      true
    )
    expect(row.contains(screen.getByRole(`button`, { name: `Send` }))).toBe(true)
  })

  it(`composites over a solid surface when opaque`, () => {
    const { container, rerender } = render(<Composer />)
    expect(card(container).className).toContain(`bg-glass-card`)
    expect(card(container).className).toContain(`border-glass-stroke-card`)
    rerender(<Composer opaque />)
    expect(card(container).className).toContain(`bg-glass-card-opaque`)
    expect(card(container).className).toContain(`border-glass-stroke-strong`)
  })
})

describe(`ComposerSubmit`, () => {
  it(`is named Send, and Stop while the agent works`, () => {
    const { rerender } = render(<ComposerSubmit />)
    expect(screen.getByRole(`button`, { name: `Send` }).title).toBe(`Send`)
    rerender(<ComposerSubmit stop />)
    const stop = screen.getByRole(`button`, { name: `Stop` })
    expect(stop.title).toBe(`Stop`)
    expect(stop.className).toContain(`text-foreground`)
  })

  it(`lets a caller override the glyph for a transient state`, () => {
    render(<ComposerSubmit>{<span>…</span>}</ComposerSubmit>)
    const button = screen.getByRole(`button`, { name: `Send` })
    expect(button.textContent).toBe(`…`)
    expect(button.querySelector(`svg`)).toBeNull()
  })
})
