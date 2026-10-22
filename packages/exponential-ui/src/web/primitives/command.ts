import { buttonVariants } from "./button"

// FEED-131 — the command menu WITHOUT cmdk: the look of the app's
// `packages/ui/src/command.tsx` (the inline variant + a trailing clear), and
// the framework-free engine both renderers share (the React set and its
// Solid twin wire the SAME store, ordering and keyboard model, so the two
// paint one DOM): cmdk's `command-score`, the filter, the root store, the
// DOM ordering pass and the keys.
export const COMMAND_CLASSES = {
  // Transparent (EXP-269): the hosts provide the glass surface.
  root: `flex h-full w-full flex-col overflow-hidden rounded-md bg-transparent text-popover-foreground`,
  inputWrapper: `flex h-9 items-center gap-2 border-b border-glass-stroke px-3`,
  inputGlyph: `size-4 shrink-0 opacity-50`,
  input: `flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-hidden placeholder:text-foreground/50 disabled:cursor-not-allowed disabled:opacity-50`,
  inputClear: buttonVariants({ variant: `ghost`, size: `icon-xs`, className: `shrink-0 text-muted-foreground` }),
  list: `max-h-[18.75rem] scroll-py-1 overflow-x-hidden overflow-y-auto`,
  empty: `py-6 text-center text-sm`,
  group: `overflow-hidden p-(--menu-surface-padding) text-foreground`,
  groupHeading: `px-(--menu-item-padding-x) py-1.5 text-xs font-medium text-muted-foreground`,
  item: `relative flex min-h-(--menu-item-height) cursor-pointer items-center gap-(--menu-item-gap) rounded-sm px-(--menu-item-padding-x) py-1 text-sm text-foreground/90 outline-hidden select-none data-[disabled=true]:cursor-default data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-50 data-[selected=true]:bg-glass-active data-[selected=true]:text-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-(--menu-icon-size) [&_svg:not([class*='text-'])]:text-muted-foreground`,
  separator: `-mx-(--menu-surface-padding) my-(--menu-surface-padding) h-px bg-glass-stroke`,
  shortcut: `ml-auto text-xs tracking-widest text-muted-foreground`,
} as const

/** cmdk's visually hidden `<label>` (its inline style, in its order). */
export const COMMAND_LABEL_STYLE = {
  position: `absolute`,
  width: `1px`,
  height: `1px`,
  padding: `0`,
  margin: `-1px`,
  overflow: `hidden`,
  clip: `rect(0, 0, 0, 0)`,
  whiteSpace: `nowrap`,
  borderWidth: `0`,
} as const

// ── command-score (cmdk 1.1.1) ──────────────────────────────────────────

const SCORE_CONTINUE_MATCH = 1
const SCORE_SPACE_WORD_JUMP = 0.9
const SCORE_NON_SPACE_WORD_JUMP = 0.8
const SCORE_CHARACTER_JUMP = 0.17
const SCORE_TRANSPOSITION = 0.1
const PENALTY_SKIPPED = 0.999
const PENALTY_CASE_MISMATCH = 0.9999
const PENALTY_NOT_COMPLETE = 0.99
const IS_GAP = /[\\/_+.#"@[({&]/
const COUNT_GAPS = /[\\/_+.#"@[({&]/g
const IS_SPACE = /[\s-]/
const COUNT_SPACE = /[\s-]/g

function scoreFrom(value: string, query: string, lowerValue: string, lowerQuery: string, valueIndex: number, queryIndex: number, memo: Record<string, number>): number {
  if (queryIndex === query.length) return valueIndex === value.length ? SCORE_CONTINUE_MATCH : PENALTY_NOT_COMPLETE
  const key = `${valueIndex},${queryIndex}`
  if (memo[key] !== undefined) return memo[key]
  const queryChar = lowerQuery.charAt(queryIndex)
  let index = lowerValue.indexOf(queryChar, valueIndex)
  let high = 0
  while (index >= 0) {
    let score = scoreFrom(value, query, lowerValue, lowerQuery, index + 1, queryIndex + 1, memo)
    if (score > high) {
      if (index === valueIndex) score *= SCORE_CONTINUE_MATCH
      else if (IS_GAP.test(value.charAt(index - 1))) {
        score *= SCORE_NON_SPACE_WORD_JUMP
        const breaks = value.slice(valueIndex, index - 1).match(COUNT_GAPS)
        if (breaks && valueIndex > 0) score *= Math.pow(PENALTY_SKIPPED, breaks.length)
      } else if (IS_SPACE.test(value.charAt(index - 1))) {
        score *= SCORE_SPACE_WORD_JUMP
        const breaks = value.slice(valueIndex, index - 1).match(COUNT_SPACE)
        if (breaks && valueIndex > 0) score *= Math.pow(PENALTY_SKIPPED, breaks.length)
      } else {
        score *= SCORE_CHARACTER_JUMP
        if (valueIndex > 0) score *= Math.pow(PENALTY_SKIPPED, index - valueIndex)
      }
      if (value.charAt(index) !== query.charAt(queryIndex)) score *= PENALTY_CASE_MISMATCH
    }
    if ((score < SCORE_TRANSPOSITION && lowerValue.charAt(index - 1) === lowerQuery.charAt(queryIndex + 1)) || (lowerQuery.charAt(queryIndex + 1) === lowerQuery.charAt(queryIndex) && lowerValue.charAt(index - 1) !== lowerQuery.charAt(queryIndex))) {
      const transposed = scoreFrom(value, query, lowerValue, lowerQuery, index + 1, queryIndex + 2, memo)
      if (transposed * SCORE_TRANSPOSITION > score) score = transposed * SCORE_TRANSPOSITION
    }
    if (score > high) high = score
    index = lowerValue.indexOf(queryChar, index + 1)
  }
  memo[key] = high
  return high
}

const normalize = (s: string) => s.toLowerCase().replace(COUNT_SPACE, ` `)

/** cmdk's `command-score`: 1 = exact, prefix > word boundary > fuzzy,
 *  0 = no match. `keywords` extend the value (`value + " " + keywords`). */
export function commandScore(value: string, query: string, keywords?: readonly string[]): number {
  const full = keywords && keywords.length > 0 ? `${value} ${keywords.join(` `)}` : value
  return scoreFrom(full, query, normalize(full), normalize(query), 0, 0, {})
}

export type CommandFilter = (value: string, query: string, keywords?: readonly string[]) => number

// ── the filter ──────────────────────────────────────────────────────────

export interface CommandFilterItem {
  id: string
  value: string
  keywords?: readonly string[]
  /** The group's id; none = an ungrouped item. */
  group?: string
  disabled?: boolean
  /** Always visible, never counted (cmdk's `forceMount`). */
  forceMount?: boolean
}

export interface CommandFilterResult {
  /** Matching (non-forced) items: 0 = the empty state. */
  count: number
  /** Ungrouped items first, then the groups by their best score; ids by score. */
  groups: { group: string | undefined; ids: string[] }[]
  scores: Map<string, number>
}

/** The visible ids per group, in display order. An empty query keeps the
 *  registration order and shows everything; otherwise score > 0 shows,
 *  items sort by score (stable), groups by their best item. */
export function filterCommandItems(items: readonly CommandFilterItem[], query: string, filter: CommandFilter = commandScore): CommandFilterResult {
  const scores = new Map<string, number>()
  const order = new Map<string | undefined, { ids: { id: string; score: number; at: number }[]; best: number; at: number }>()
  let count = 0
  items.forEach((item, at) => {
    const score = item.forceMount ? 1 : query ? (item.value ? filter(item.value, query, item.keywords) : 0) : 1
    scores.set(item.id, score)
    if (!item.forceMount && score > 0) count++
    if (score <= 0) return
    let g = order.get(item.group)
    if (!g) order.set(item.group, (g = { ids: [], best: 0, at }))
    g.ids.push({ id: item.id, score, at })
    g.best = Math.max(g.best, score)
  })
  const groups = [...order.entries()].map(([group, g]) => ({ group, ...g }))
  if (query) {
    for (const g of groups) g.ids.sort((a, b) => b.score - a.score || a.at - b.at)
    groups.sort((a, b) => (a.group === undefined ? -1 : b.group === undefined ? 1 : b.best - a.best || a.at - b.at))
  } else groups.sort((a, b) => (a.group === undefined ? -1 : b.group === undefined ? 1 : a.at - b.at))
  return { count, groups: groups.map((g) => ({ group: g.group, ids: g.ids.map((i) => i.id) })), scores }
}

// ── the root store ──────────────────────────────────────────────────────

export const COMMAND_SELECT_EVENT = `command-item-select`
const ITEM = `[data-slot="command-item"]`
const ENABLED_ITEM = `${ITEM}:not([aria-disabled="true"])`
const GROUP = `[data-slot="command-group"]`

type Kind = `item` | `group` | `separator`
interface Entry {
  id: string
  kind: Kind
  seq: number
  el: HTMLElement | null
  value: string
  keywords?: string[]
  group?: string
  disabled?: boolean
  forceMount?: boolean
}

export interface CommandStoreOptions {
  /** The root's live props. */
  props: () => { value?: string; onValueChange?: (value: string) => void; filter?: CommandFilter; shouldFilter?: boolean; loop?: boolean }
  /** Ask the renderer for a flush after the DOM caught up (React: a root
   *  re-render + layout effect; Solid: a microtask). */
  schedule: () => void
}

/** The command root's state (cmdk's store), framework-free: the query, the
 *  selected value, the registry (items, groups, separators in creation
 *  order) and the post-render pass (`flush`: DOM order, first-item
 *  selection, the active descendant, scroll). Renderers subscribe and
 *  re-render on `emit`. */
export function createCommandStore(o: CommandStoreOptions) {
  let search = ``
  let value = o.props().value?.trim() ?? ``
  let selectedId: string | undefined
  let seq = 0
  let version = 0
  const entries = new Map<string, Entry>()
  const byEl = new WeakMap<Element, Entry>()
  const listeners = new Set<() => void>()
  let sizer: HTMLElement | null = null
  let dirtyOrder = false
  let selectFirst = false
  let scroll = false
  let selectionChanged = false

  const filter = () => o.props().filter ?? commandScore
  const filtering = () => search !== `` && o.props().shouldFilter !== false
  const emit = () => {
    version++
    listeners.forEach((l) => l())
  }
  const touch = () => {
    o.schedule()
  }
  const items = () => [...entries.values()].filter((e) => e.kind === `item`).sort((a, b) => a.seq - b.seq)
  const result = () => filterCommandItems(items(), filtering() ? search : ``, filter())

  const setValue = (next: string, opts: { pointer?: boolean } = {}) => {
    const p = o.props()
    if (next === (p.value !== undefined ? p.value.trim() : value)) return
    if (p.value === undefined) value = next
    p.onValueChange?.(next)
    if (!opts.pointer) scroll = true
    selectionChanged = true
    emit()
    touch()
  }

  const enabledItems = () => Array.from(sizer?.querySelectorAll<HTMLElement>(ENABLED_ITEM) ?? [])
  const selectedEl = () => sizer?.querySelector<HTMLElement>(`${ITEM}[aria-selected="true"]`) ?? null

  /** Registered children of one container, in the display order. */
  const reorder = (container: Element, scores: Map<string, number>) => {
    const kids = Array.from(container.children)
      .map((el) => byEl.get(el))
      .filter((e): e is Entry => e !== undefined)
    if (kids.length === 0) return
    const groupScore = (g: Entry) => Math.max(0, ...items().filter((i) => i.group === g.id).map((i) => scores.get(i.id) ?? 0))
    let want: Entry[]
    if (!filtering()) want = [...kids].sort((a, b) => a.seq - b.seq)
    else {
      const rank = (e: Entry) => (e.kind === `item` ? 1 : e.kind === `group` ? 2 : 0)
      const score = (e: Entry) => (e.kind === `item` ? (scores.get(e.id) ?? 0) : e.kind === `group` ? groupScore(e) : 0)
      want = [...kids].sort((a, b) => rank(a) - rank(b) || score(b) - score(a) || a.seq - b.seq)
    }
    if (want.every((e, i) => e === kids[i])) return
    for (const e of want) if (e.el) container.appendChild(e.el)
  }

  return {
    get version() {
      return version
    },
    get search() {
      return search
    },
    get value() {
      const p = o.props().value
      return p !== undefined ? p.trim() : value
    },
    get selectedId() {
      return selectedId
    },
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
    nextSeq: () => seq++,
    /** The query (the input); `silent` = during the first render. */
    setSearch(next: string, silent = false) {
      if (next === search) return
      search = next
      dirtyOrder = true
      selectFirst = true
      if (silent) return
      emit()
      touch()
    },
    setValue,
    /** Re-read the controlled `value` prop (no callback). */
    syncValue() {
      const p = o.props().value
      if (p === undefined || p.trim() === value) return false
      value = p.trim()
      selectionChanged = true
      scroll = true
      emit()
      touch()
      return true
    },
    setSizer(el: HTMLElement | null) {
      sizer = el
    },
    filtering,
    /** An item's score now (1 when nothing filters). */
    score(itemValue: string, keywords?: readonly string[]) {
      if (!filtering()) return 1
      return itemValue ? filter()(itemValue, search, keywords) : 0
    },
    /** Whether a group shows (some registered item of it matches). */
    groupVisible(groupId: string) {
      if (!filtering()) return true
      return items().some((i) => i.group === groupId && this.score(i.value, i.keywords) > 0)
    },
    /** The empty state: no matching (non-forced) item. */
    empty() {
      return result().count === 0
    },
    register(entry: { id: string; kind: Kind; seq: number; value?: string; keywords?: string[]; group?: string; disabled?: boolean; forceMount?: boolean }) {
      const e: Entry = { el: null, value: ``, ...entry }
      entries.set(e.id, e)
      dirtyOrder = true
      if (e.kind === `item` && !this.value) selectFirst = true
      emit()
      touch()
      return () => {
        entries.delete(e.id)
        if (e.el) byEl.delete(e.el)
        dirtyOrder = true
        if (e.kind === `item` && selectedId === e.id) selectFirst = true
        emit()
        touch()
      }
    },
    /** An entry's element (null = not rendered) or item fields. */
    update(id: string, patch: Partial<Pick<Entry, `el` | `value` | `keywords` | `disabled`>>) {
      const e = entries.get(id)
      if (!e) return
      let changed = false
      if (`el` in patch && patch.el !== e.el) {
        if (e.el) byEl.delete(e.el)
        e.el = patch.el ?? null
        if (e.el) byEl.set(e.el, e)
        dirtyOrder = true
        changed = true
      }
      if (patch.value !== undefined && patch.value !== e.value) {
        e.value = patch.value
        dirtyOrder = true
        if (!this.value) selectFirst = true
        changed = true
      }
      if (`keywords` in patch && (patch.keywords ?? []).join(`\u0000`) !== (e.keywords ?? []).join(`\u0000`)) {
        e.keywords = patch.keywords
        dirtyOrder = true
        changed = true
      }
      if (`disabled` in patch && patch.disabled !== e.disabled) {
        e.disabled = patch.disabled
        changed = true
      }
      if (!changed) return
      emit()
      touch()
    },
    /** The post-render pass: DOM order, the first item selected (after a
     *  query change or when nothing is selected), the active descendant,
     *  the selected item scrolled into view. Idempotent. */
    flush() {
      if (!sizer) return
      if (dirtyOrder) {
        dirtyOrder = false
        const { scores } = result()
        reorder(sizer, scores)
        for (const items of Array.from(sizer.querySelectorAll(`[data-slot="command-group-items"]`))) reorder(items, scores)
      }
      if (selectFirst) {
        selectFirst = false
        const first = enabledItems()[0]
        const next = first?.getAttribute(`data-value`) ?? ``
        if (next !== this.value) {
          setValue(next)
          return
        }
      }
      const sel = selectedEl()
      const id = sel?.id || undefined
      if (id !== selectedId) {
        selectedId = id
        emit()
      }
      if (selectionChanged) {
        selectionChanged = false
        if (scroll && sel) {
          if (sel.parentElement?.firstElementChild === sel) sel.closest(GROUP)?.querySelector(`[data-slot="command-group-heading"]`)?.scrollIntoView({ block: `nearest` })
          sel.scrollIntoView({ block: `nearest` })
        }
        scroll = false
      }
    },
    /** cmdk's root keys. */
    onKeyDown(e: KeyboardEvent) {
      if (e.defaultPrevented || e.isComposing || e.keyCode === 229) return
      const loop = Boolean(o.props().loop)
      const pick = (el: HTMLElement | undefined) => {
        if (el) setValue(el.getAttribute(`data-value`) ?? ``)
      }
      const at = (i: number) => pick(enabledItems()[i])
      const last = () => at(enabledItems().length - 1)
      const step = (delta: number) => {
        const list = enabledItems()
        const i = list.findIndex((el) => el === selectedEl())
        let next = list[i + delta]
        if (loop) next = i + delta < 0 ? list[list.length - 1] : i + delta === list.length ? list[0] : list[i + delta]
        pick(next)
      }
      const group = (delta: number) => {
        let g: Element | null | undefined = selectedEl()?.closest(GROUP)
        let found: HTMLElement | null | undefined
        while (g && !found) {
          g = delta > 0 ? nextMatching(g, GROUP, 1) : nextMatching(g, GROUP, -1)
          found = g?.querySelector<HTMLElement>(ENABLED_ITEM)
        }
        if (found) pick(found)
        else step(delta)
      }
      const down = () => {
        e.preventDefault()
        if (e.metaKey) last()
        else if (e.altKey) group(1)
        else step(1)
      }
      const up = () => {
        e.preventDefault()
        if (e.metaKey) at(0)
        else if (e.altKey) group(-1)
        else step(-1)
      }
      switch (e.key) {
        case `n`:
        case `j`:
          if (e.ctrlKey) down()
          break
        case `ArrowDown`:
          down()
          break
        case `p`:
        case `k`:
          if (e.ctrlKey) up()
          break
        case `ArrowUp`:
          up()
          break
        case `Home`:
          e.preventDefault()
          at(0)
          break
        case `End`:
          e.preventDefault()
          last()
          break
        case `Enter`: {
          e.preventDefault()
          selectedEl()?.dispatchEvent(new Event(COMMAND_SELECT_EVENT))
          break
        }
      }
    },
  }
}

export type CommandStore = ReturnType<typeof createCommandStore>

function nextMatching(el: Element, selector: string, dir: 1 | -1): Element | null {
  let n = dir > 0 ? el.nextElementSibling : el.previousElementSibling
  while (n) {
    if (n.matches(selector)) return n
    n = dir > 0 ? n.nextElementSibling : n.previousElementSibling
  }
  return null
}

/** An item's value: the `value` prop, else string children, else the
 *  rendered text (trimmed), else the last known one. */
export function commandItemValue(value: string | undefined, children: unknown, el: Element | null | undefined, known: string): string {
  if (value !== undefined) return value.trim()
  if (typeof children === `string`) return children.trim()
  if (el) return (el.textContent ?? ``).trim()
  return known
}

/** CommandList's `--cmdk-list-height` (cmdk: the sizer's offsetHeight,
 *  ResizeObserver → rAF). Returns the disposer. */
export function observeCommandListHeight(list: HTMLElement, sizer: HTMLElement): () => void {
  if (typeof ResizeObserver === `undefined`) return () => {}
  let raf = 0
  const ro = new ResizeObserver(() => {
    raf = requestAnimationFrame(() => list.style.setProperty(`--cmdk-list-height`, `${sizer.offsetHeight.toFixed(1)}px`))
  })
  ro.observe(sizer)
  return () => {
    cancelAnimationFrame(raf)
    ro.unobserve(sizer)
  }
}
