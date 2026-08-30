import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"

// EXP-687 — the mobile chrome contract. These assertions are about CLASSES
// rather than pixels because jsdom has no layout: the `max-sm:` arms are the
// spec (one bottom sheet with a grabber and no ✕, a compact centered alert,
// a full-screen page only where a lightbox needs one), and a regression shows
// up here as a missing or extra utility.

function panel(slot: string) {
  const node = document.querySelector(`[data-slot="${slot}"]`)
  expect(node, `no ${slot} rendered`).toBeTruthy()
  return node as HTMLElement
}

const grabber = () => document.querySelector(`[data-slot="sheet-grabber"]`)

describe(`DialogContent mobile arms`, () => {
  it(`defaults to the bottom sheet: a grabber, and a ✕ hidden on phones`, () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Sheet</DialogTitle>
        </DialogContent>
      </Dialog>
    )

    const content = panel(`dialog-content`)
    expect(content.getAttribute(`data-mobile`)).toBe(`sheet`)
    expect(content.className).toContain(`max-sm:bg-glass-bottom`)
    expect(content.className).toContain(`max-sm:rounded-t-3xl`)
    // Content-fitted, not the fixed 94dvh detent.
    expect(content.className).toContain(`max-sm:max-h-[90dvh]`)
    expect(grabber()).toBeTruthy()

    const close = screen.getByText(`Close`).closest(`button`)!
    expect(close.className).toContain(`max-sm:hidden`)
  })

  it(`sheet-full takes the fixed detent`, () => {
    render(
      <Dialog open>
        <DialogContent mobile="sheet-full">
          <DialogTitle>Tall</DialogTitle>
        </DialogContent>
      </Dialog>
    )
    const content = panel(`dialog-content`)
    expect(content.getAttribute(`data-mobile`)).toBe(`sheet-full`)
    expect(content.className).toContain(`max-sm:h-[94dvh]`)
  })

  it(`alert is a compact centered panel with a visible Cancel and no grabber`, () => {
    render(
      <Dialog open>
        <DialogContent mobile="alert">
          <DialogTitle>Delete?</DialogTitle>
          <DialogFooter>
            <DialogCancel />
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )

    const content = panel(`dialog-content`)
    expect(content.className).toContain(`max-sm:max-w-sm`)
    expect(content.className).not.toContain(`max-sm:bottom-0`)
    expect(grabber()).toBeNull()

    // Native alert parity: the alert arm is the one that keeps Cancel.
    const cancel = screen.getByText(`Cancel`)
    expect(cancel.className).not.toContain(`max-sm:hidden`)
  })

  it(`hides DialogCancel on a phone for the sheet arms`, () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Sheet</DialogTitle>
          <DialogFooter>
            <DialogCancel />
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    expect(screen.getByText(`Cancel`).className).toContain(`max-sm:hidden`)
  })

  it(`page keeps its ✕ and covers the viewport`, () => {
    render(
      <Dialog open>
        <DialogContent mobile="page">
          <DialogTitle>Lightbox</DialogTitle>
        </DialogContent>
      </Dialog>
    )
    const content = panel(`dialog-content`)
    expect(content.className).toContain(`max-sm:inset-0`)
    expect(grabber()).toBeNull()
    expect(
      screen.getByText(`Close`).closest(`button`)!.className
    ).not.toContain(`max-sm:hidden`)
  })

  it(`the footer turns every action into the full-width lg capsule on phones`, () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Sheet</DialogTitle>
          <DialogFooter />
        </DialogContent>
      </Dialog>
    )
    expect(panel(`dialog-footer`).className).toContain(
      `max-sm:[&>[data-slot=button]]:w-full`
    )
  })
})

describe(`AlertDialogContent`, () => {
  it(`is the compact centered alert, never a full-screen page`, () => {
    render(
      <AlertDialog open>
        <AlertDialogContent>
          <AlertDialogTitle>Move issue</AlertDialogTitle>
        </AlertDialogContent>
      </AlertDialog>
    )
    const content = panel(`alert-dialog-content`)
    expect(content.className).toContain(`max-sm:max-w-sm`)
    expect(content.className).toContain(`max-sm:bg-glass-bottom`)
    // The old arm was `fixed inset-0 ... bg-background` — a whole screen for a
    // yes/no question.
    expect(content.className).not.toContain(`inset-0`)
  })
})

describe(`SheetContent`, () => {
  it(`a bottom sheet gets the grabber and drops the ✕`, () => {
    render(
      <Sheet open>
        <SheetContent side="bottom">
          <SheetTitle>Boards</SheetTitle>
        </SheetContent>
      </Sheet>
    )
    expect(grabber()).toBeTruthy()
    expect(screen.queryByText(`Close`)).toBeNull()
    expect(panel(`sheet-content`).className).toContain(`bg-glass-bottom`)
  })

  it(`showGrabber={false} makes it page-like`, () => {
    render(
      <Sheet open>
        <SheetContent side="bottom" showGrabber={false}>
          <SheetTitle>Search</SheetTitle>
        </SheetContent>
      </Sheet>
    )
    expect(grabber()).toBeNull()
  })

  it(`a side drawer keeps its ✕ and its frosted glass`, () => {
    render(
      <Sheet open>
        <SheetContent side="right">
          <SheetTitle>Inbox</SheetTitle>
        </SheetContent>
      </Sheet>
    )
    expect(grabber()).toBeNull()
    expect(screen.getByText(`Close`)).toBeTruthy()
    expect(panel(`sheet-content`).className).toContain(`backdrop-blur-2xl`)
  })
})
