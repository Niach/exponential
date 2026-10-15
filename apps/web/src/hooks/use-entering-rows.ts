import { useRef } from "react"

// EXP-1323: rows animate between bands (Needs you · Running · Recent) on the
// motion tokens. A row keyed by its band remounts when it moves; this names
// the rows that were NOT there when the list first painted, so only a row
// that arrives later enters (`motion-enter`) and the first paint stays still.

/** The `motion-enter` class for a row key that arrived after the first
 *  paint, else ``. */
export function useEnteringRows(keys: readonly string[]): (key: string) => string {
  const initial = useRef<Set<string> | null>(null)
  if (initial.current === null && keys.length > 0) initial.current = new Set(keys)
  const seen = initial.current
  return (key) => (seen && !seen.has(key) ? `motion-enter` : ``)
}
