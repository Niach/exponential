import * as React from "react"

import { cn } from "../cn"

// N-14 (picker-presentation.json): a ROW host that offers an inline slot.
// The row's inline pickers keep their collapsed triggers exactly as they
// are and portal their rows into ONE slot rendered under the whole row, as
// wide as the row, so the trigger and its siblings never move. The slot
// holds one picker at a time: opening another replaces the rows.

export interface InlinePickerRowValue {
  /** The slot element under the row (null until mounted). */
  slot: HTMLElement | null
  /** The picker whose rows are in the slot, if any. */
  openId: string | null
  claim: (id: string) => void
  release: (id: string) => void
}

export const InlinePickerRowContext =
  React.createContext<InlinePickerRowValue | null>(null)

/** The row host: `children` is the row itself; the slot sits under it. */
export function InlinePickerRow({
  children,
  className,
  slotClassName,
}: {
  children: React.ReactNode
  className?: string
  slotClassName?: string
}) {
  const [slot, setSlot] = React.useState<HTMLElement | null>(null)
  const [openId, setOpenId] = React.useState<string | null>(null)
  const claim = React.useCallback((id: string) => setOpenId(id), [])
  const release = React.useCallback(
    (id: string) => setOpenId((current) => (current === id ? null : current)),
    []
  )
  const value = React.useMemo(
    () => ({ slot, openId, claim, release }),
    [slot, openId, claim, release]
  )
  return (
    <InlinePickerRowContext.Provider value={value}>
      <div data-slot="inline-picker-row" className={cn(`flex min-w-0 flex-col`, className)}>
        {children}
        <div
          ref={setSlot}
          data-slot="inline-picker-slot"
          data-state={openId === null ? `closed` : `open`}
          className={cn(`min-w-0 empty:hidden`, slotClassName)}
        />
      </div>
    </InlinePickerRowContext.Provider>
  )
}
