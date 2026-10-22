// FEED-131: the Solid Tabs' Radix behaviour (mouse down / focus select,
// roving focus, disabled triggers, panels).
import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs"

const tree = (onValueChange = vi.fn()) => (
  <Tabs defaultValue="a" onValueChange={onValueChange}>
    <TabsList>
      <TabsTrigger value="a">Account</TabsTrigger>
      <TabsTrigger value="b">Password</TabsTrigger>
      <TabsTrigger value="c" disabled>
        Billing
      </TabsTrigger>
    </TabsList>
    <TabsContent value="a">Account panel</TabsContent>
    <TabsContent value="b">Password panel</TabsContent>
  </Tabs>
)

describe(`Tabs`, () => {
  it(`selects on mouse down and shows only the active panel`, () => {
    const onValueChange = vi.fn()
    render(() => tree(onValueChange))
    expect(screen.getByText(`Account panel`)).toBeTruthy()
    expect(screen.queryByText(`Password panel`)).toBeNull()
    fireEvent.mouseDown(screen.getByRole(`tab`, { name: `Password` }), { button: 0 })
    expect(onValueChange).toHaveBeenCalledWith(`b`)
    expect(screen.getByText(`Password panel`)).toBeTruthy()
    expect(screen.getByRole(`tab`, { name: `Password` }).getAttribute(`aria-selected`)).toBe(`true`)
    const hidden = document.getElementById(screen.getByRole(`tab`, { name: `Account` }).getAttribute(`aria-controls`)!)!
    expect(hidden.hidden).toBe(true)
  })

  it(`a disabled trigger never selects`, () => {
    const onValueChange = vi.fn()
    render(() => tree(onValueChange))
    fireEvent.mouseDown(screen.getByRole(`tab`, { name: `Billing` }), { button: 0 })
    expect(onValueChange).not.toHaveBeenCalled()
  })

  it(`arrows rove the focus (skipping disabled, looping) and focus selects`, async () => {
    const onValueChange = vi.fn()
    render(() => tree(onValueChange))
    const account = screen.getByRole(`tab`, { name: `Account` })
    account.focus()
    fireEvent.keyDown(account, { key: `ArrowRight` })
    await new Promise((r) => setTimeout(r, 0))
    expect(document.activeElement?.textContent).toBe(`Password`)
    expect(onValueChange).toHaveBeenCalledWith(`b`)
    fireEvent.keyDown(document.activeElement!, { key: `ArrowRight` })
    await new Promise((r) => setTimeout(r, 0))
    expect(document.activeElement?.textContent).toBe(`Account`)
  })

  it(`the list is the tab stop until a trigger holds it`, () => {
    render(() => tree())
    expect(screen.getByRole(`tablist`).getAttribute(`tabindex`)).toBe(`0`)
    for (const t of screen.getAllByRole(`tab`)) expect(t.getAttribute(`tabindex`)).toBe(`-1`)
    screen.getByRole(`tab`, { name: `Account` }).focus()
    expect(screen.getByRole(`tab`, { name: `Account` }).getAttribute(`tabindex`)).toBe(`0`)
  })
})
