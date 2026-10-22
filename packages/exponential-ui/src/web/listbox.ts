// Round 1 (a11y.json Select / TimePicker / ChipInput suggestions; FEED-128:
// hoisted from the React renderer's natives/listbox.ts): the LISTBOX
// keyboard model's arithmetic. ArrowUp/ArrowDown move the active option
// (wrapping, skipping disabled), Home/End jump, printable keys type-ahead by
// label (a 700 ms buffer; one repeated letter cycles).

/** The TYPE-AHEAD buffer resets after this pause (ms). */
export const TYPEAHEAD_RESET_MS = 700

/** The next enabled index `delta` steps from `from` (wrapping), or -1. */
export function nextEnabledIndex(count: number, from: number, delta: number, isDisabled: (index: number) => boolean = () => false): number {
  if (count === 0) return -1
  let i = from
  for (let n = 0; n < count; n++) {
    i = (((i + delta) % count) + count) % count
    if (!isDisabled(i)) return i
  }
  return -1
}

export interface TypeaheadBuffer {
  text: string
  at: number
}

/** One printable key typed: the new buffer and the index it lands on (-1 =
 *  no label starts with the buffer; the active option stays). */
export function typeaheadStep(buffer: TypeaheadBuffer, key: string, now: number, active: number, count: number, isDisabled: (index: number) => boolean, labelOf: (index: number) => string): { buffer: TypeaheadBuffer; index: number } {
  const next = { text: now - buffer.at > TYPEAHEAD_RESET_MS ? key.toLowerCase() : buffer.text + key.toLowerCase(), at: now }
  for (let n = 1; n <= count; n++) {
    const i = (Math.max(0, active) + (next.text.length > 1 ? n - 1 : n)) % count
    if (!isDisabled(i) && labelOf(i).toLowerCase().startsWith(next.text)) return { buffer: next, index: i }
  }
  return { buffer: next, index: -1 }
}
