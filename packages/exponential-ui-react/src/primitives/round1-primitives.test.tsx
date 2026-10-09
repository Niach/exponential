// Round 1: the shadcn primitives added to the set (AlertDialog, Breadcrumb,
// Kbd, ScrollArea) render with their slots and Radix behaviour.

import { describe, expect, it } from "vitest"
import { fireEvent, render, screen } from "@testing-library/react"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogTitle, AlertDialogTrigger } from "./alert-dialog"
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "./breadcrumb"
import { Kbd, KbdGroup } from "./kbd"
import { ScrollArea } from "./scroll-area"

describe(`round-1 primitives`, () => {
  it(`AlertDialog opens from its trigger as an alertdialog with both actions`, () => {
    render(
      <AlertDialog>
        <AlertDialogTrigger>Delete</AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogTitle>Delete board?</AlertDialogTitle>
          <AlertDialogDescription>This cannot be undone.</AlertDialogDescription>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction>Delete</AlertDialogAction>
        </AlertDialogContent>
      </AlertDialog>
    )
    fireEvent.click(screen.getByText(`Delete`))
    expect(screen.getByRole(`alertdialog`)).toBeTruthy()
    expect(document.activeElement?.textContent).toBe(`Cancel`)
  })
  it(`Breadcrumb, Kbd, ScrollArea render their slots`, () => {
    const { container } = render(
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
        <ScrollArea className="h-10">body</ScrollArea>
      </>
    )
    expect(container.querySelector(`[aria-current="page"]`)!.textContent).toBe(`Board`)
    expect(container.querySelectorAll(`kbd[data-slot="kbd"]`).length).toBe(2)
    expect(container.querySelector(`[data-slot="scroll-area-viewport"]`)!.textContent).toBe(`body`)
  })
})
