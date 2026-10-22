// Round 1 (React's round1-primitives.test.tsx, the overlay part, on the
// Solid twin): AlertDialog opens from its trigger as an alertdialog with
// both actions, the focus on Cancel; no outside dismissal, Escape cancels.

import { describe, expect, it } from "vitest"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogTitle, AlertDialogTrigger } from "./alert-dialog"

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function renderAlert(onOpenChange?: (open: boolean) => void) {
  return render(() => (
    <AlertDialog onOpenChange={onOpenChange}>
      <AlertDialogTrigger>Delete</AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogTitle>Delete board?</AlertDialogTitle>
        <AlertDialogDescription>This cannot be undone.</AlertDialogDescription>
        <AlertDialogCancel>Cancel</AlertDialogCancel>
        <AlertDialogAction>Delete</AlertDialogAction>
      </AlertDialogContent>
    </AlertDialog>
  ))
}

describe(`round-1 primitives`, () => {
  it(`AlertDialog opens from its trigger as an alertdialog with both actions`, async () => {
    renderAlert()
    fireEvent.click(screen.getByText(`Delete`))
    await tick()
    expect(screen.getByRole(`alertdialog`)).toBeTruthy()
    expect(document.activeElement?.textContent).toBe(`Cancel`)
  })

  it(`ignores a pointer down outside, Escape and Cancel close it`, async () => {
    const changes: boolean[] = []
    renderAlert((open) => changes.push(open))
    fireEvent.click(screen.getByText(`Delete`))
    await tick()
    fireEvent.pointerDown(document.body)
    expect(screen.queryByRole(`alertdialog`)).toBeTruthy()
    fireEvent.keyDown(document.activeElement!, { key: `Escape` })
    expect(screen.queryByRole(`alertdialog`)).toBeNull()
    fireEvent.click(screen.getByText(`Delete`))
    await tick()
    fireEvent.click(screen.getByText(`Cancel`))
    expect(screen.queryByRole(`alertdialog`)).toBeNull()
    expect(changes).toEqual([true, false, true, false])
  })
})
