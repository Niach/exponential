// FEED-131: Radix's typeahead (`useTypeaheadSearch` + `getNextMatch`): a
// 1 s buffer, a repeated letter cycles through the matches. `getNextMatch`
// takes the labels (Menu), `findNextItem` the items (Select).

import { onCleanup } from "solid-js"

export const wrapArray = <T,>(array: T[], start: number) => array.map((_, i) => array[(start + i) % array.length])

export function getNextMatch(values: string[], search: string, currentMatch?: string): string | undefined {
  const isRepeated = search.length > 1 && Array.from(search).every((c) => c === search[0])
  const normalized = isRepeated ? search[0] : search
  const currentIndex = currentMatch ? values.indexOf(currentMatch) : -1
  let wrapped = wrapArray(values, Math.max(currentIndex, 0))
  if (normalized.length === 1) wrapped = wrapped.filter((v) => v !== currentMatch)
  const next = wrapped.find((v) => v.toLowerCase().startsWith(normalized.toLowerCase()))
  return next !== currentMatch ? next : undefined
}

/** Radix's Select type-ahead: a 1 s buffer, a repeated letter cycles. */
export function radixTypeahead(onSearch: (search: string) => void) {
  let search = ``
  let timer = 0
  const update = (v: string) => {
    search = v
    window.clearTimeout(timer)
    if (v !== ``) timer = window.setTimeout(() => update(``), 1000)
  }
  onCleanup(() => window.clearTimeout(timer))
  return {
    get search() {
      return search
    },
    type: (key: string) => {
      const next = search + key
      onSearch(next)
      update(next)
    },
    reset: () => {
      search = ``
      window.clearTimeout(timer)
    },
  }
}

export function findNextItem<T extends { label: string }>(items: T[], search: string, current: T | undefined): T | undefined {
  const repeated = search.length > 1 && Array.from(search).every((c) => c === search[0])
  const normalized = repeated ? search[0] : search
  const at = current ? items.indexOf(current) : -1
  let wrapped = items.map((_, i) => items[(Math.max(at, 0) + i) % items.length])
  if (normalized.length === 1) wrapped = wrapped.filter((v) => v !== current)
  const next = wrapped.find((item) => item.label.toLowerCase().startsWith(normalized.toLowerCase()))
  return next !== current ? next : undefined
}
