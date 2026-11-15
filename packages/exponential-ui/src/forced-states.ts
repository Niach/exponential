// #EXP-1781: FORCED interaction states, a HOST-ONLY option (never in the
// catalog or the A2UI schema): the host pins `hover` / `pressed` / `focus`
// on a node so a catalogue specimen can show what a pointer or a keyboard
// would paint. PAINT ONLY: forced states feed the style resolution (recipe
// `when.state`, a node's `:hover` / `:focus-visible` / `:pressed` blocks)
// and nothing else: no event, no hover-opened overlay, no focus move, no
// accessibility change. They are a union over the real interaction (a real
// pointer leave never clears a forced hover). `fixtures/forced-states.json`
// is the spec; the core (`eui_core::forced`) mirrors this file.

/** The states a host may force. */
export const FORCED_STATE_NAMES = [`hover`, `pressed`, `focus`] as const
export type ForcedStateName = (typeof FORCED_STATE_NAMES)[number]

/** id → the states forced on it (a node id, or a placed part id such as
 *  `tabs.tab.1`; a key covers that node AND its parts, ids `<key>.…`). */
export type ForcedStates = Readonly<Record<string, readonly string[]>>

/** The normalized map (`focus` = a KEYBOARD focus: `focus` + `focus-visible`)
 *  and the refused entries (`<id>:<state>`, in input order). */
export function normalizeForcedStates(input: ForcedStates | undefined | null): { forced: Record<string, string[]>; dropped: string[] } {
  const forced: Record<string, string[]> = {}
  const dropped: string[] = []
  for (const [id, states] of Object.entries(input ?? {})) {
    const out: string[] = []
    for (const s of states ?? []) {
      if (!(FORCED_STATE_NAMES as readonly string[]).includes(s)) {
        dropped.push(`${id}:${s}`)
        continue
      }
      for (const x of s === `focus` ? [`focus`, `focus-visible`] : [s]) if (!out.includes(x)) out.push(x)
    }
    if (out.length) forced[id] = out
  }
  return { forced, dropped }
}

/** The forced states node or part `id` carries: the union of every key
 *  that is `id` or a dotted prefix of it, in FORCED order (hover, pressed,
 *  focus, focus-visible). */
export function forcedStatesOf(forced: Readonly<Record<string, readonly string[]>>, id: string): string[] {
  const got = new Set<string>()
  for (const [key, states] of Object.entries(forced)) if (id === key || id.startsWith(`${key}.`)) for (const s of states) got.add(s)
  return [`hover`, `pressed`, `focus`, `focus-visible`].filter((s) => got.has(s))
}
