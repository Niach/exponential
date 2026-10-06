import { act, fireEvent, render, screen } from "@testing-library/react"
import { useState } from "react"
import { describe, expect, it, vi } from "vitest"
import { Dialog } from "./dialog"
import { Prompt, PromptLayout, promptFocusIndex, type PromptAction } from "./prompt"

// EXP-1215 — the one confirm/choice prompt: role, order, focus, dismissal
// and the async row. jsdom has no layout, so the shape is asserted by roles
// and classes.

function names() {
  return screen
    .getAllByRole(`button`)
    .map((button) => button.textContent ?? ``)
}

// Radix attaches its document pointerdown listener on a 0ms timer.
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

/** A node outside the card, i.e. the scrim's side of a pointer-down. */
function outsideNode() {
  const node = document.createElement(`div`)
  document.body.appendChild(node)
  return node
}

const button = (label: string) =>
  screen.getByText(label).closest(`button`) as HTMLButtonElement

describe(`Prompt`, () => {
  it(`is an alert dialog with the title, the body and the actions in order`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Delete this file?"
        body="report.pdf is removed for everyone."
        data-testid="file-delete"
        actions={[
          { label: `Cancel` },
          { label: `Delete`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    const dialog = screen.getByRole(`alertdialog`)
    expect(dialog.getAttribute(`data-testid`)).toBe(`file-delete`)
    expect(dialog.getAttribute(`data-mobile`)).toBe(`alert`)
    expect(screen.getByText(`Delete this file?`)).toBeTruthy()
    expect(screen.getByText(`report.pdf is removed for everyone.`)).toBeTruthy()
    expect(names()).toEqual([`Cancel`, `Delete`])
    // No ✕, ever.
    expect(screen.queryByText(`Close`)).toBeNull()
    // Every answer is the 32px md pill.
    for (const button of screen.getAllByRole(`button`)) {
      expect(button.getAttribute(`data-slot`)).toBe(`pill`)
      expect(button.getAttribute(`data-size`)).toBe(`md`)
    }
    expect(screen.getByText(`Delete`).className).toContain(`text-destructive`)
    expect(screen.getByText(`Delete`).className).not.toContain(`bg-primary`)
  })

  it(`focuses Cancel in a plain destructive confirm, never the destructive answer`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Delete issue?"
        actions={[
          { label: `Cancel` },
          { label: `Delete`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    expect(document.activeElement?.textContent).toBe(`Cancel`)
  })

  it(`focuses the primary, sets the quiet destructive apart on the leading edge`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Save this issue as a draft?"
        actions={[
          { label: `Discard`, role: `quietDestructive`, onSelect: () => {} },
          { label: `Create issue`, onSelect: () => {} },
          { label: `Save draft`, role: `primary`, onSelect: () => {} },
        ]}
      />
    )
    expect(document.activeElement?.textContent).toBe(`Save draft`)
    expect(screen.getByText(`Save draft`).className).toContain(`bg-primary`)
    expect(screen.getByText(`Discard`).className).toContain(`mr-auto`)
    expect(names()).toEqual([`Discard`, `Create issue`, `Save draft`])
  })

  it(`focus rule: autoFocus wins, never on a destructive or disabled action`, () => {
    const actions: PromptAction[] = [
      { label: `A`, role: `destructive`, autoFocus: true },
      { label: `B`, disabled: true },
      { label: `C` },
      { label: `D`, role: `primary` },
    ]
    expect(promptFocusIndex(actions)).toBe(3)
    expect(promptFocusIndex([{ label: `A` }, { label: `B`, autoFocus: true }])).toBe(1)
    expect(promptFocusIndex([{ label: `X`, role: `destructive` }])).toBe(-1)
  })

  it(`dismisses on Esc and on an action without onSelect`, () => {
    const onOpenChange = vi.fn()
    const onDismiss = vi.fn()
    render(
      <Prompt
        open
        onOpenChange={onOpenChange}
        onDismiss={onDismiss}
        title="Remove member?"
        actions={[
          { label: `Cancel` },
          { label: `Remove`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    fireEvent.keyDown(screen.getByRole(`alertdialog`), { key: `Escape` })
    expect(onDismiss).toHaveBeenCalledTimes(1)
    expect(onOpenChange).toHaveBeenLastCalledWith(false)
    fireEvent.click(screen.getByText(`Cancel`))
    expect(onDismiss).toHaveBeenCalledTimes(2)
  })

  it(`keeps the card open with the row disabled while an async action runs`, async () => {
    let resolve: () => void = () => {}
    const onOpenChange = vi.fn()
    function Host() {
      const [open, setOpen] = useState(true)
      return (
        <Prompt
          open={open}
          onOpenChange={(next) => {
            onOpenChange(next)
            setOpen(next)
          }}
          title="Merge pull request?"
          actions={[
            { label: `Cancel` },
            {
              label: `Merge`,
              role: `primary`,
              onSelect: () =>
                new Promise<void>((r) => {
                  resolve = r
                }),
            },
          ]}
        />
      )
    }
    render(<Host />)
    fireEvent.click(screen.getByText(`Merge`))
    expect((screen.getByText(`Merge`) as HTMLButtonElement).disabled).toBe(true)
    expect((screen.getByText(`Cancel`) as HTMLButtonElement).disabled).toBe(true)
    // Esc is ignored while it runs.
    fireEvent.keyDown(screen.getByRole(`alertdialog`), { key: `Escape` })
    expect(onOpenChange).not.toHaveBeenCalled()
    expect(screen.getByRole(`alertdialog`)).toBeTruthy()
    await act(async () => {
      resolve()
    })
    expect((screen.getByText(`Merge`) as HTMLButtonElement).disabled).toBe(false)
  })

  it(`renders the content slot between the text and the row`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Start a blocked issue?"
        actions={[{ label: `Cancel` }]}
      >
        <div data-testid="slot">graph</div>
      </Prompt>
    )
    const slot = screen.getByTestId(`slot`)
    const title = screen.getByText(`Start a blocked issue?`)
    const cancel = screen.getByText(`Cancel`)
    expect(
      title.compareDocumentPosition(slot) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    expect(
      slot.compareDocumentPosition(cancel) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
  })

  it(`stacks the row only when told it cannot fit: default on top, quiet at the bottom`, () => {
    render(
      <Dialog open>
        <PromptLayout
          stacked
          title="Save this issue as a draft?"
          actions={[
            { label: `Discard`, role: `quietDestructive` },
            { label: `Create issue` },
            { label: `Save draft`, role: `primary` },
          ]}
        />
      </Dialog>
    )
    const row = document.querySelector(`[data-prompt-actions]`) as HTMLElement
    expect(row.hasAttribute(`data-stacked`)).toBe(true)
    // flex-col-reverse: the DOM keeps reading order, the card paints it
    // bottom-up, so Save draft is on top.
    expect(row.className).toContain(`flex-col-reverse`)
    expect(row.className).toContain(`items-end`)
    // Natural-width pills, every one trailing-aligned: the quiet answer
    // drops its leading-edge offset in the stack.
    const discard = screen.getByText(`Discard`)
    expect(discard.className).not.toContain(`mr-auto`)
    expect(discard.className).toContain(`text-destructive`)
    for (const button of screen.getAllByRole(`button`)) {
      expect(button.className).not.toContain(`w-full`)
    }
  })

  it(`keeps one row by default (jsdom has no layout, so it always fits)`, () => {
    render(
      <Prompt open onOpenChange={() => {}} title="Q?" actions={[{ label: `Cancel` }]} />
    )
    const row = document.querySelector(`[data-prompt-actions]`) as HTMLElement
    expect(row.hasAttribute(`data-stacked`)).toBe(false)
    // Never a two-row hybrid: the row does not wrap, it stacks as a whole.
    expect(row.className).toContain(`flex-nowrap`)
  })

  it(`a busy answer keeps its label and shows the spinner; the row locks`, () => {
    const onOpenChange = vi.fn()
    render(
      <Prompt
        open
        onOpenChange={onOpenChange}
        title={`Delete "Bug"?`}
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: `Delete`, role: `destructive`, busy: true, onSelect: () => {} },
        ]}
      />
    )
    const del = screen.getByText(`Delete`).closest(`button`) as HTMLButtonElement
    expect(del.textContent).toBe(`Delete`)
    expect(del.hasAttribute(`data-busy`)).toBe(true)
    expect(del.querySelector(`[data-prompt-spinner]`)).not.toBeNull()
    expect(del.disabled).toBe(true)
    expect(
      (screen.getByText(`Cancel`).closest(`button`) as HTMLButtonElement).disabled
    ).toBe(true)
    fireEvent.keyDown(screen.getByRole(`alertdialog`), { key: `Escape` })
    expect(onOpenChange).not.toHaveBeenCalled()
  })

  it(`an async answer spins only its own pill while it runs`, async () => {
    let resolve: () => void = () => {}
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Merge PR #7?"
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: `Merge`,
            role: `primary`,
            onSelect: () =>
              new Promise<void>((r) => {
                resolve = r
              }),
          },
        ]}
      />
    )
    fireEvent.click(screen.getByText(`Merge`))
    const merge = screen.getByText(`Merge`).closest(`button`) as HTMLElement
    expect(merge.querySelector(`[data-prompt-spinner]`)).not.toBeNull()
    expect(
      screen.getByText(`Cancel`).closest(`button`)?.querySelector(`[data-prompt-spinner]`)
    ).toBeNull()
    await act(async () => {
      resolve()
    })
    expect(merge.querySelector(`[data-prompt-spinner]`)).toBeNull()
  })

  it(`the cancel role is a plain pill that dismisses and takes focus in a destructive confirm`, () => {
    const onOpenChange = vi.fn()
    render(
      <Prompt
        open
        onOpenChange={onOpenChange}
        title="Stop this run?"
        actions={[
          { label: `Cancel`, role: `cancel`, autoFocus: true },
          { label: `Stop`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    expect(document.activeElement?.textContent).toBe(`Cancel`)
    expect(screen.getByText(`Cancel`).className).not.toContain(`bg-primary`)
    fireEvent.click(screen.getByText(`Cancel`))
    expect(onOpenChange).toHaveBeenLastCalledWith(false)
  })

  it(`a pointer-down on the scrim takes the cancel path`, async () => {
    const onOpenChange = vi.fn()
    const onDismiss = vi.fn()
    render(
      <Prompt
        open
        onOpenChange={onOpenChange}
        onDismiss={onDismiss}
        title="Remove device?"
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: `Remove`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    await tick()
    fireEvent.pointerDown(outsideNode())
    expect(onDismiss).toHaveBeenCalledTimes(1)
    expect(onOpenChange).toHaveBeenLastCalledWith(false)
  })

  it(`the prompt-level busy locks the row, Esc and the scrim`, async () => {
    const onOpenChange = vi.fn()
    const onDismiss = vi.fn()
    const onSelect = vi.fn()
    render(
      <Prompt
        open
        busy
        onOpenChange={onOpenChange}
        onDismiss={onDismiss}
        title="Delete team?"
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: `Delete`, role: `destructive`, onSelect },
        ]}
      />
    )
    await tick()
    expect(button(`Cancel`).disabled).toBe(true)
    expect(button(`Delete`).disabled).toBe(true)
    fireEvent.click(button(`Delete`))
    expect(onSelect).not.toHaveBeenCalled()
    fireEvent.keyDown(screen.getByRole(`alertdialog`), { key: `Escape` })
    fireEvent.pointerDown(outsideNode())
    expect(onDismiss).not.toHaveBeenCalled()
    expect(onOpenChange).not.toHaveBeenCalled()
    expect(screen.getByRole(`alertdialog`)).toBeTruthy()
  })

  it(`Tab stays trapped inside the card`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Stop this run?"
        actions={[
          { label: `Cancel`, role: `cancel` },
          { label: `Stop`, role: `destructive`, onSelect: () => {} },
        ]}
      />
    )
    const dialog = screen.getByRole(`alertdialog`)
    expect(document.activeElement).toBe(button(`Cancel`))
    // The trap acts at the edges (between them the browser moves focus,
    // which jsdom does not): Tab on the last answer wraps to the first,
    // Shift+Tab on the first wraps to the last. Focus never leaves the card.
    button(`Stop`).focus()
    fireEvent.keyDown(dialog, { key: `Tab` })
    expect(document.activeElement).toBe(button(`Cancel`))
    fireEvent.keyDown(dialog, { key: `Tab`, shiftKey: true })
    expect(document.activeElement).toBe(button(`Stop`))
    expect(dialog.contains(document.activeElement)).toBe(true)
  })

  it(`re-enables the row after an async answer rejects`, async () => {
    let reject: (error: Error) => void = () => {}
    const onOpenChange = vi.fn()
    render(
      <Prompt
        open
        onOpenChange={onOpenChange}
        title="Merge PR #9?"
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: `Merge`,
            role: `primary`,
            onSelect: () =>
              new Promise<void>((_, r) => {
                reject = r
              }),
          },
        ]}
      />
    )
    fireEvent.click(button(`Merge`))
    expect(button(`Merge`).disabled).toBe(true)
    expect(button(`Cancel`).disabled).toBe(true)
    expect(button(`Merge`).querySelector(`[data-prompt-spinner]`)).not.toBeNull()
    await act(async () => {
      reject(new Error(`refused`))
    })
    // The refusal is the site's to report; the card stays open and live.
    expect(button(`Merge`).disabled).toBe(false)
    expect(button(`Cancel`).disabled).toBe(false)
    expect(button(`Merge`).querySelector(`[data-prompt-spinner]`)).toBeNull()
    expect(onOpenChange).not.toHaveBeenCalled()
    expect(screen.getByRole(`alertdialog`)).toBeTruthy()
  })

  it(`a content-slot field marked data-prompt-autofocus takes focus first`, () => {
    render(
      <Prompt
        open
        onOpenChange={() => {}}
        title="Delete Acme?"
        actions={[{ label: `Cancel` }, { label: `Delete team`, role: `destructive` }]}
      >
        <input aria-label="Team name" data-prompt-autofocus />
      </Prompt>
    )
    expect(document.activeElement?.getAttribute(`aria-label`)).toBe(`Team name`)
  })
})
