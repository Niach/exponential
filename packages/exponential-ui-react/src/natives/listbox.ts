// Round 1 (a11y.json Select / TimePicker / ChipInput suggestions): the
// LISTBOX keyboard model the hand-built pickers share — an active option
// (aria-activedescendant, focus stays where it is), ArrowUp/ArrowDown move
// it (wrapping, skipping disabled), Home/End jump, Enter picks, printable
// keys type-ahead by label when the list owns focus.

import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react"

export interface ListNavOptions {
  count: number
  isDisabled?: (index: number) => boolean
  labelOf?: (index: number) => string
  onPick: (index: number) => void
  /** Type-ahead (off when a search field owns the keys). */
  typeahead?: boolean
  initial?: number
}

export function useListNavigation({ count, isDisabled = () => false, labelOf = () => ``, onPick, typeahead = false, initial = -1 }: ListNavOptions) {
  const [active, setActive] = useState(initial)
  const buffer = useRef({ text: ``, at: 0 })
  useEffect(() => {
    if (active >= count) setActive(count - 1)
  }, [count, active])
  const move = useCallback(
    (from: number, delta: number) => {
      if (count === 0) return -1
      let i = from
      for (let n = 0; n < count; n++) {
        i = (((i + delta) % count) + count) % count
        if (!isDisabled(i)) return i
      }
      return -1
    },
    [count, isDisabled]
  )
  const onKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === `ArrowDown`) {
        e.preventDefault()
        setActive((a) => move(a < 0 ? -1 : a, 1))
      } else if (e.key === `ArrowUp`) {
        e.preventDefault()
        setActive((a) => move(a < 0 ? 0 : a, -1))
      } else if (e.key === `Home`) {
        e.preventDefault()
        setActive(move(-1, 1))
      } else if (e.key === `End`) {
        e.preventDefault()
        setActive(move(0, -1))
      } else if (e.key === `Enter` || (e.key === ` ` && typeahead && buffer.current.text === ``)) {
        if (active >= 0 && active < count && !isDisabled(active)) {
          e.preventDefault()
          onPick(active)
        }
      } else if (typeahead && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
        const now = Date.now()
        buffer.current = { text: now - buffer.current.at > 700 ? e.key.toLowerCase() : buffer.current.text + e.key.toLowerCase(), at: now }
        for (let n = 1; n <= count; n++) {
          const i = (Math.max(0, active) + (buffer.current.text.length > 1 ? n - 1 : n)) % count
          if (!isDisabled(i) && labelOf(i).toLowerCase().startsWith(buffer.current.text)) {
            setActive(i)
            break
          }
        }
      }
    },
    [move, active, count, isDisabled, onPick, typeahead, labelOf]
  )
  return { active, setActive, onKeyDown }
}
