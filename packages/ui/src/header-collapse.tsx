import { headerCollapse, type HeaderCollapseChip, type HeaderCollapsible } from "@exp/domain-contract/header-actions"
import { createContext, useCallback, useContext, useLayoutEffect, useState, type ReactNode } from "react"
import { cn } from "./cn"

/** #EXP-2183 (header-actions.json COLLAPSE): the trailing chips a narrow
 *  header folds to their glyph; empty = nothing folds (every wide header). */
export const HeaderFoldContext = createContext<ReadonlySet<HeaderCollapsible>>(new Set())

export function useHeaderFolded(action: HeaderCollapsible): boolean {
  return useContext(HeaderFoldContext).has(action)
}

/** A foldable chip's label: out of sight while folded (still read out). A
 *  `gap` (px) the label takes from the chip besides its own width. */
export function HeaderFoldLabel({ folded, gap, children }: { folded: boolean; gap?: number; children: ReactNode }) {
  return (
    <span data-header-fold-label data-gap={gap} className={cn(folded && `sr-only`)}>
      {children}
    </span>
  )
}

const px = (value: string) => Number.parseFloat(value) || 0

/** Measures a header band: the row (`[data-slot=mobile-detail-header-row]`),
 *  the title (`[data-header-title]`, its row's siblings never fold) and the
 *  chips (`[data-header-fold=<action>]` with a `HeaderFoldLabel`), then
 *  folds by `headerCollapse`. Pass the result to `HeaderFoldContext`. */
export function useHeaderCollapse(): { ref: (node: HTMLElement | null) => void; folded: ReadonlySet<HeaderCollapsible> } {
  const [band, setBand] = useState<HTMLElement | null>(null)
  const [folded, setFolded] = useState<ReadonlySet<HeaderCollapsible>>(() => new Set())
  const ref = useCallback((node: HTMLElement | null) => setBand(node), [])

  useLayoutEffect(() => {
    if (!band) return
    const measure = () => {
      const row = band.querySelector<HTMLElement>(`[data-slot=mobile-detail-header-row]`)
      const title = band.querySelector<HTMLElement>(`[data-header-title]`)
      if (!row || !title) return
      const titleRow = title.parentElement
      let titleSpace = title.clientWidth
      if (titleRow) {
        const gap = px(getComputedStyle(titleRow).columnGap)
        const siblings = [...titleRow.children].filter((child) => child !== title) as HTMLElement[]
        titleSpace = titleRow.clientWidth - siblings.reduce((sum, child) => sum + child.offsetWidth + gap, 0)
      }
      let placed = 0
      const chips: HeaderCollapseChip[] = []
      for (const el of band.querySelectorAll<HTMLElement>(`[data-header-fold]`)) {
        const action = el.dataset.headerFold as HeaderCollapsible
        const label = el.querySelector<HTMLElement>(`[data-header-fold-label]`)
        const width = el.offsetWidth
        placed += width
        if (!label) {
          chips.push({ action, full: width, compact: width })
          continue
        }
        const gap = label.dataset.gap !== undefined ? px(label.dataset.gap) : px(getComputedStyle(label.parentElement ?? label).columnGap)
        const isFolded = label.classList.contains(`sr-only`)
        chips.push(
          isFolded
            ? { action, full: width + label.scrollWidth + gap, compact: width }
            : { action, full: width, compact: width - label.offsetWidth - gap }
        )
      }
      const bar = row.clientWidth
      const next = headerCollapse({ bar, title: title.scrollWidth, fixed: bar - Math.max(0, titleSpace) - placed, chips })
      setFolded((current) =>
        current.size === next.length && next.every((action) => current.has(action)) ? current : new Set(next)
      )
    }
    measure()
    const resize = new ResizeObserver(measure)
    resize.observe(band)
    for (const el of band.querySelectorAll(`[data-header-fold], [data-header-title]`)) resize.observe(el)
    const mutate = new MutationObserver(() => {
      for (const el of band.querySelectorAll(`[data-header-fold], [data-header-title]`)) resize.observe(el)
      measure()
    })
    mutate.observe(band, { subtree: true, childList: true, characterData: true })
    return () => {
      resize.disconnect()
      mutate.disconnect()
    }
  }, [band])

  return { ref, folded }
}
