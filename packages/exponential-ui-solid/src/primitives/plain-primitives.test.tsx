// The parts of React's round1-primitives.test.tsx this lane owns
// (Breadcrumb, Kbd), plus the plain DOM Radix parts (Separator, Progress,
// Label) whose behaviour the twins re-implement.

import { describe, expect, it } from "vitest"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "./breadcrumb"
import { Kbd, KbdGroup } from "./kbd"
import { Label } from "./label"
import { Progress } from "./progress"
import { Separator } from "./separator"

describe(`plain primitives`, () => {
  it(`Breadcrumb and Kbd render their slots`, () => {
    const { container } = render(() => (
      <>
        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem>
              <BreadcrumbLink href="/">Home</BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator />
            <BreadcrumbItem>
              <BreadcrumbPage>Board</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
        <KbdGroup>
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </KbdGroup>
      </>
    ))
    expect(container.querySelector(`[aria-current="page"]`)!.textContent).toBe(`Board`)
    expect(container.querySelector(`[data-slot="breadcrumb-separator"] svg`)!.getAttribute(`class`)).toContain(`lucide-chevron-right`)
    expect(container.querySelectorAll(`kbd[data-slot="kbd"]`).length).toBe(2)
  })

  it(`Separator: decorative = role none, semantic vertical = aria-orientation`, () => {
    const { container } = render(() => (
      <>
        <Separator />
        <Separator orientation="vertical" decorative={false} />
      </>
    ))
    const [a, b] = Array.from(container.querySelectorAll(`[data-slot="separator"]`))
    expect(a!.getAttribute(`role`)).toBe(`none`)
    expect(a!.getAttribute(`data-orientation`)).toBe(`horizontal`)
    expect(b!.getAttribute(`role`)).toBe(`separator`)
    expect(b!.getAttribute(`aria-orientation`)).toBe(`vertical`)
  })

  it(`Progress: a progressbar with Radix's value attributes`, () => {
    render(() => <Progress value={42} />)
    const bar = screen.getByRole(`progressbar`)
    expect(bar.getAttribute(`aria-valuenow`)).toBe(`42`)
    expect(bar.getAttribute(`aria-valuetext`)).toBe(`42%`)
    expect(bar.getAttribute(`data-state`)).toBe(`loading`)
    expect((bar.firstElementChild as HTMLElement).style.transform).toBe(`translateX(-58%)`)
  })

  it(`Label: a double-click never selects its text, a press on a control inside passes`, () => {
    render(() => (
      <Label>
        Name <input />
      </Label>
    ))
    const label = screen.getByText(/Name/)
    const double = new MouseEvent(`mousedown`, { bubbles: true, cancelable: true, detail: 2 })
    label.dispatchEvent(double)
    expect(double.defaultPrevented).toBe(true)
    const onInput = new MouseEvent(`mousedown`, { bubbles: true, cancelable: true, detail: 2 })
    label.querySelector(`input`)!.dispatchEvent(onInput)
    expect(onInput.defaultPrevented).toBe(false)
    fireEvent.mouseDown(label, { detail: 1 })
  })
})
