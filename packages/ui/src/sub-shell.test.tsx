import { fireEvent, render, screen } from "@testing-library/react"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import { GlassGroup, GlassRow } from "./glass-rows"
import { SubShell, SubShellHost } from "./sub-shell"

// EXP-1029 contract, implemented by EXP-1020 — sub-shell navigation. The
// IDE, iOS and Android siblings carry the same case names.
//
// "Gone" is asserted on the `hidden` attribute of the level's body, not on
// absence from the DOM: the card stays MOUNTED behind the page on purpose,
// so a row's live props (a picker value, a draft) keep reaching the page it
// opened. Hidden is hidden for layout, for the pointer and for a11y.

function card() {
  return (
    <SubShellHost>
      <GlassGroup>
        <GlassRow>Card row</GlassRow>
        <SubShell label="Workflow settings" value="opus · fable">
          <GlassGroup>
            <GlassRow>Child page row</GlassRow>
          </GlassGroup>
        </SubShell>
      </GlassGroup>
    </SubShellHost>
  )
}

const bodyOf = (container: HTMLElement, slot: string) =>
  container.querySelector(`[data-slot="${slot}"] > [data-slot="sub-shell-body"]`)

describe(`SubShell (EXP-1029 contract)`, () => {
  it(`renders as a row inside the card, with its label, value and chevron`, () => {
    const { container } = render(card())
    expect(container.querySelector(`[data-slot="sub-shell-host"]`)).not.toBeNull()
    const row = container.querySelector(`[data-slot="sub-shell"]`)
    expect(row).not.toBeNull()
    expect(screen.getByText(`Workflow settings`)).toBeTruthy()
    expect(screen.getByText(`opus · fable`)).toBeTruthy()
    // At rest the child page is not in the document.
    expect(screen.queryByText(`Child page row`)).toBeNull()
  })
})

describe(`SubShell navigation (EXP-1029 → EXP-1020)`, () => {
  it(`clicking the row slides the child page in place of the WHOLE card`, () => {
    const { container } = render(card())
    fireEvent.click(screen.getByText(`Workflow settings`))
    expect(container.querySelector(`[data-slot="sub-shell-page"]`)).not.toBeNull()
    expect(screen.getByText(`Child page row`)).toBeTruthy()
    expect(bodyOf(container, `sub-shell-host`)?.hasAttribute(`hidden`)).toBe(true)
  })

  it(`the child page carries a back button on top that returns to the card`, () => {
    const { container } = render(card())
    fireEvent.click(screen.getByText(`Workflow settings`))
    const back = screen.getByLabelText(`Back`)
    // On top: the back button is the page body's first child.
    expect(bodyOf(container, `sub-shell-page`)?.firstElementChild?.contains(back)).toBe(true)
    fireEvent.click(back)
    expect(container.querySelector(`[data-slot="sub-shell-page"]`)).toBeNull()
    expect(bodyOf(container, `sub-shell-host`)?.hasAttribute(`hidden`)).toBe(false)
  })

  it(`the child page is the same shell: its own GlassGroups of rows`, () => {
    const { container } = render(card())
    fireEvent.click(screen.getByText(`Workflow settings`))
    const page = container.querySelector(`[data-slot="sub-shell-page"]`)
    expect(page?.querySelector(`[data-slot="glass-group"]`)).not.toBeNull()
  })

  it(`a sub-shell inside the child page slides one level deeper, back returns one level`, () => {
    const { container } = render(
      <SubShellHost>
        <GlassGroup>
          <SubShell label="Workflow settings">
            <GlassGroup>
              <GlassRow>Child page row</GlassRow>
              <SubShell label="Advanced">
                <GlassGroup>
                  <GlassRow>Deep row</GlassRow>
                </GlassGroup>
              </SubShell>
            </GlassGroup>
          </SubShell>
        </GlassGroup>
      </SubShellHost>
    )
    fireEvent.click(screen.getByText(`Workflow settings`))
    fireEvent.click(screen.getByText(`Advanced`))
    expect(screen.getByText(`Deep row`)).toBeTruthy()
    expect(container.querySelectorAll(`[data-slot="sub-shell-page"]`)).toHaveLength(2)
    // The deeper page hides its parent's rows AND its parent's back header:
    // exactly ONE back button is visible.
    const backs = screen.getAllByLabelText(`Back`)
    expect(backs.filter((button) => button.closest(`[hidden]`) === null)).toHaveLength(1)
    fireEvent.click(backs[backs.length - 1]!)
    expect(screen.queryByText(`Deep row`)).toBeNull()
    expect(screen.getByText(`Child page row`)).toBeTruthy()
  })

  it(`the host's other groups are gone while a page is open — never a card inside a card`, () => {
    const { container } = render(card())
    fireEvent.click(screen.getByText(`Workflow settings`))
    const hostBody = bodyOf(container, `sub-shell-host`)
    expect(hostBody?.hasAttribute(`hidden`)).toBe(true)
    // "Card row" is still mounted (live props keep flowing) but hidden.
    expect(screen.getByText(`Card row`).closest(`[hidden]`)).toBe(hostBody)
    // The page is a sibling of the hidden body, not nested inside it.
    expect(hostBody?.contains(screen.getByText(`Child page row`))).toBe(false)
  })

  it(`controlled open/onOpenChange drives the page from outside`, () => {
    function Controlled() {
      const [open, setOpen] = useState(false)
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Open from outside
          </button>
          <SubShellHost>
            <GlassGroup>
              <SubShell label="Workflow settings" open={open} onOpenChange={setOpen}>
                <GlassGroup>
                  <GlassRow>Child page row</GlassRow>
                </GlassGroup>
              </SubShell>
            </GlassGroup>
          </SubShellHost>
        </>
      )
    }
    render(<Controlled />)
    expect(screen.queryByText(`Child page row`)).toBeNull()
    fireEvent.click(screen.getByText(`Open from outside`))
    expect(screen.getByText(`Child page row`)).toBeTruthy()
    fireEvent.click(screen.getByLabelText(`Back`))
    expect(screen.queryByText(`Child page row`)).toBeNull()
  })

  it(`Escape (pointer) and swipe back (phone) return to the card`, () => {
    const { container } = render(card())
    fireEvent.click(screen.getByText(`Workflow settings`))
    fireEvent.keyDown(screen.getByText(`Child page row`), { key: `Escape` })
    expect(container.querySelector(`[data-slot="sub-shell-page"]`)).toBeNull()

    fireEvent.click(screen.getByText(`Workflow settings`))
    const body = bodyOf(container, `sub-shell-page`)!
    fireEvent.touchStart(body, { touches: [{ clientX: 10, clientY: 40 }] })
    fireEvent.touchEnd(body, { changedTouches: [{ clientX: 140, clientY: 48 }] })
    expect(container.querySelector(`[data-slot="sub-shell-page"]`)).toBeNull()
  })

  it(`a disabled row never opens`, () => {
    const { container } = render(
      <SubShellHost>
        <GlassGroup>
          <SubShell label="Workflow settings" disabled>
            <GlassGroup>
              <GlassRow>Child page row</GlassRow>
            </GlassGroup>
          </SubShell>
        </GlassGroup>
      </SubShellHost>
    )
    fireEvent.click(screen.getByText(`Workflow settings`))
    expect(container.querySelector(`[data-slot="sub-shell-page"]`)).toBeNull()
    expect(screen.queryByText(`Child page row`)).toBeNull()
  })

  it(`keeps the open page in sync with the card's live props`, () => {
    function Live() {
      const [model, setModel] = useState(`opus`)
      return (
        <SubShellHost>
          <GlassGroup>
            <SubShell label="Workflow settings" value={model}>
              <GlassGroup>
                <GlassRow>Model: {model}</GlassRow>
                <button type="button" onClick={() => setModel(`fable`)}>
                  Pick fable
                </button>
              </GlassGroup>
            </SubShell>
          </GlassGroup>
        </SubShellHost>
      )
    }
    render(<Live />)
    fireEvent.click(screen.getByText(`Workflow settings`))
    expect(screen.getByText(`Model: opus`)).toBeTruthy()
    fireEvent.click(screen.getByText(`Pick fable`))
    expect(screen.getByText(`Model: fable`)).toBeTruthy()
  })
})
