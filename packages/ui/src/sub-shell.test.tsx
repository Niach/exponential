import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { GlassGroup, GlassRow } from "./glass-rows"
import { SubShell, SubShellHost } from "./sub-shell"

// EXP-1029 contract — sub-shell navigation. The live case pins the stub's
// row; the skipped table is the behaviour EXP-1020 implements and un-skips
// (the IDE, iOS and Android siblings carry the same case names).

describe(`SubShell (EXP-1029 contract)`, () => {
  it(`renders as a row inside the card, with its label, value and chevron`, () => {
    const { container } = render(
      <SubShellHost>
        <GlassGroup>
          <SubShell label="Workflow settings" value="opus · fable">
            <GlassGroup>
              <GlassRow>Child page row</GlassRow>
            </GlassGroup>
          </SubShell>
        </GlassGroup>
      </SubShellHost>
    )
    expect(container.querySelector(`[data-slot="sub-shell-host"]`)).not.toBeNull()
    const row = container.querySelector(`[data-slot="sub-shell"]`)
    expect(row).not.toBeNull()
    expect(screen.getByText(`Workflow settings`)).toBeTruthy()
    expect(screen.getByText(`opus · fable`)).toBeTruthy()
    // At rest the child page is not in the document.
    expect(screen.queryByText(`Child page row`)).toBeNull()
  })
})

describe.skip(`SubShell navigation (EXP-1029 → EXP-1020)`, () => {
  it(`clicking the row slides the child page in place of the WHOLE card`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`the child page carries a back button on top that returns to the card`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`the child page is the same shell: its own GlassGroups of rows`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`a sub-shell inside the child page slides one level deeper, back returns one level`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`the host's other groups are gone while a page is open — never a card inside a card`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`controlled open/onOpenChange drives the page from outside`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`Escape (pointer) and swipe back (phone) return to the card`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
  it(`a disabled row never opens`, () => {
    expect.fail(`EXP-1020 implements this case`)
  })
})
