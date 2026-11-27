// #EXP-2204 (React's natives/ci.tsx; docs/ci-components-contract.md §1–§5,
// a11y.json LogLines + JobGraph): the CI pieces. LogLines = LogView's
// toolbar (search, `n of m`, prev/next, Follow, Timestamps) over the
// WINDOWED log (fixed-height rows of `logRows`: folding step rows, numbered
// lines with pressable anchors, the ansi tokens, the search marks);
// JobGraph = the stages as columns (`jobGraphLayout`), the needs of the
// hovered or focused node as cubic connectors, a status list below `md`.
// The rules are the core's, the painting helpers web/ci.ts: the DOM is
// React's, element for element.

import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack, type Component, type JSX } from "solid-js"
import { JOB_GRAPH, ciStatusEntry, extendLineAnchor, jobGraphLayout, jobGraphUpstream, logModel, logOpenDefault, logReveal, logRows, logSearch, parseLineAnchor, resolveRecipe, tokenizeCode, type JobGraphNode, type LogMatch } from "@exponential_app/eui"
import { CI_FOLLOW_SLACK, LOG_VIEW, ciBottom, ciColumns, ciCompact, ciOffsets, ciRevealOffset, ciSegments, ciWindow, type CiToken } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { arr, bool, css, Hidden, num, parts, str, type Attrs } from "./shared"

/** A recipe length of the theme (a number), else `fallback`. */
function recipeLength(theme: NativeProps[`theme`], mode: NativeProps[`mode`], component: string, part: string, key: string, fallback: number): number {
  const v = resolveRecipe(theme, { component, part }, mode)[key]
  return typeof v === `number` ? v : fallback
}

const numbers = (v: unknown): number[] => (Array.isArray(v) ? v.filter((x): x is number => typeof x === `number` && Number.isInteger(x)) : [])

/** The status glyph (§1): the table's icon, spinning while running. */
function StatusGlyph(props: { part: Attrs; status: unknown }): JSX.Element {
  const ctx = useSurfaceContext()
  const entry = () => ciStatusEntry(props.status)
  return (
    <span {...props.part} data-xui-ci-spin={entry().spin ? `` : undefined}>
      <IconGlyph icons={ctx.host.icons} name={entry().icon} />
    </span>
  )
}

export function LogLinesNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const node = () => props.node
  const scope = () => props.scope
  const model = createMemo(() => logModel(arr<unknown>(props.props.lines), arr<unknown>(props.props.sections)))
  const [open, setOpen] = createBoundState<number[]>(node, scope, `open`, () => (Array.isArray(props.props.open) ? numbers(props.props.open) : logOpenDefault(model())))
  const [query, setQuery] = createBoundState<string>(node, scope, `query`, () => str(props.props.query))
  const [selection, setSelection] = createBoundState<string>(node, scope, `selection`, () => str(props.props.selection))
  const [follow, setFollow] = createBoundState<boolean>(node, scope, `follow`, () => (props.props.follow === undefined || props.props.follow === null ? true : bool(props.props.follow)))
  const [timestamps, setTimestamps] = createBoundState<boolean>(node, scope, `timestamps`, () => bool(props.props.timestamps))
  const maxLines = () => (props.props.maxLines === undefined || props.props.maxLines === null ? LOG_VIEW.maxLines : Math.max(1, num(props.props.maxLines)))

  // Geometry: the theme's line height and step row height (fixed rows).
  const lineH = () => recipeLength(props.theme, props.mode, `LogLines`, `line`, `lineHeight`, 22.75)
  const sectionH = () => recipeLength(props.theme, props.mode, `LogLines`, `section`, `height`, 40)
  const pad = () => recipeLength(props.theme, props.mode, `LogLines`, `body`, `paddingVertical`, 0)
  const rows = createMemo(() => logRows(model(), open()))
  const extents = createMemo(() => rows().map((r) => (r.kind === `section` ? sectionH() : lineH())))
  const offsets = createMemo(() => ciOffsets(extents()))
  const total = () => offsets()[rows().length] ?? 0
  const cols = createMemo(() => ciColumns(model().lines.map((l) => l.text)))
  const gutterWidth = () => `${String(model().lines.length).length + 1}ch`

  // Search.
  const matches = createMemo(() => logSearch(model(), query()))
  const byLine = createMemo(() => {
    const map = new Map<number, { first: number; list: LogMatch[] }>()
    matches().forEach((m, i) => {
      const hit = map.get(m.line)
      if (hit) hit.list.push(m)
      else map.set(m.line, { first: i, list: [m] })
    })
    return map
  })
  const [currentRaw, setCurrent] = createSignal(0)
  const current = () => (matches().length ? Math.min(currentRaw(), matches().length - 1) : -1)
  const range = createMemo(() => parseLineAnchor(selection()))

  // The ansi tokens, per line, built when a line first shows.
  const tokens = createMemo(() => (model(), new Map<number, CiToken[]>()))
  const tokensOf = (n: number): CiToken[] => {
    const cache = tokens()
    let hit = cache.get(n)
    if (!hit) {
      hit = (tokenizeCode(model().lines[n - 1]?.text ?? ``, `ansi`)[0] ?? []) as CiToken[]
      cache.set(n, hit)
    }
    return hit
  }

  // The window: the body's scroll and size (content coordinates).
  let body: HTMLDivElement | undefined
  const initialViewport = () => maxLines() * lineH() + 2 * pad()
  const [view, setView] = createSignal<{ scroll: number; viewport: number; width: number }>({ scroll: 0, viewport: untrack(initialViewport), width: 0 })
  const viewport = () => view().viewport || initialViewport()
  const win = createMemo(() => ciWindow(extents(), offsets(), view().scroll - pad(), viewport()), undefined, { equals: (a, b) => a.start === b.start && a.end === b.end && a.before === b.before && a.after === b.after })
  let lastTop = 0
  let ignoreTop: number | null = null
  const scrollTo = (top: number) => {
    if (body) body.scrollTop = top
    ignoreTop = top
    lastTop = top
    setView((v) => (v.scroll === top ? v : { ...v, scroll: top }))
  }
  const turnFollow = (next: boolean) => {
    setFollow(next)
    void props.emit(`follow`, { follow: next }, { follow: next })
  }
  const onScroll = () => {
    const el = body
    if (!el) return
    const top = el.scrollTop
    const programmatic = ignoreTop !== null && Math.abs(top - ignoreTop) < 1
    ignoreTop = null
    if (!programmatic && untrack(follow) && top < lastTop - 1 && el.scrollHeight - top - el.clientHeight > CI_FOLLOW_SLACK) turnFollow(false)
    lastTop = top
    setView((v) => (v.scroll === top ? v : { ...v, scroll: top }))
  }
  onMount(() => {
    const el = body
    if (!el || typeof ResizeObserver === `undefined`) return
    const read = () => setView((v) => (v.viewport === (el.clientHeight || v.viewport) && v.width === el.clientWidth ? v : { ...v, viewport: el.clientHeight || v.viewport, width: el.clientWidth }))
    read()
    const ro = new ResizeObserver(read)
    ro.observe(el)
    onCleanup(() => ro.disconnect())
  })
  // Follow: stick to the bottom as lines arrive (and when switched on).
  createEffect(
    on([follow, total, viewport], ([following]) => {
      if (following) scrollTo(ciBottom(untrack(total) + 2 * untrack(pad), untrack(viewport)))
    })
  )

  // Reveal: open the line's section, then scroll it in (top third).
  let pending: { line: number; always: boolean } | null = null
  const [revealTick, setRevealTick] = createSignal(0)
  const reveal = (line: number, always: boolean) => {
    const cur = untrack(open)
    const next = logReveal(untrack(model), cur, line)
    if (next.length !== cur.length || next.some((v, i) => v !== cur[i])) setOpen(next)
    pending = { line, always }
    setRevealTick((t) => t + 1)
  }
  createEffect(
    on([rows, revealTick], ([list]) => {
      const p = pending
      if (!p) return
      const at = list.findIndex((r) => r.kind === `line` && r.n === p.line)
      if (at < 0) return
      pending = null
      untrack(() => scrollTo(ciRevealOffset((offsets()[at] ?? 0) + pad(), lineH(), view().scroll, viewport(), total() + 2 * pad(), p.always)))
    })
  )
  // A selection that is new from outside (or the first) reveals its start
  // and stops the follow; a press on a number does neither.
  let own: string | null = null
  createEffect(
    on(selection, (sel) => {
      if (sel === own) return
      own = sel
      const r = parseLineAnchor(sel)
      if (!r) return
      untrack(() => {
        reveal(r.start, true)
        if (follow()) turnFollow(false)
      })
    })
  )

  const toggle = (index: number) => {
    const cur = open()
    const was = cur.includes(index)
    const next = was ? cur.filter((v) => v !== index) : [...cur, index].sort((a, b) => a - b)
    setOpen(next)
    void props.emit(`toggle`, { section: index, open: !was }, { open: next })
  }
  const search = (q: string) => {
    setQuery(q)
    setCurrent(0)
    void props.emit(`search`, { query: q }, { query: q })
    const first = logSearch(model(), q)[0]
    if (first) reveal(first.line, false)
  }
  const step = (by: number) => {
    const list = matches()
    if (!list.length) return
    const c = current()
    const next = ((((c < 0 ? 0 : c) + by) % list.length) + list.length) % list.length
    setCurrent(next)
    reveal(list[next]!.line, false)
  }
  const press = (n: number, shift: boolean) => {
    const anchor = extendLineAnchor(selection(), n, shift)
    const r = parseLineAnchor(anchor)!
    own = anchor
    setSelection(anchor)
    void props.emit(`select`, { anchor, start: r.start, end: r.end }, { selection: anchor })
    if (follow()) turnFollow(false)
  }

  // The built rows, keyed like React's (`s<index>` / `l<n>`).
  const keys = createMemo(
    () => {
      const w = win()
      const list = rows()
      const out: string[] = []
      for (let i = w.start; i < w.end; i++) {
        const r = list[i]!
        out.push(r.kind === `section` ? `s${r.section}` : `l${r.n}`)
      }
      return out
    },
    undefined,
    { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) }
  )

  const SectionRow = (p: { index: number }) => {
    const s = () => model().sections[p.index]!
    const isOpen = () => open().includes(p.index)
    return (
      <button type="button" {...part.atWith(`section`, p.index, { status: s().status, open: isOpen() })} aria-expanded={isOpen() ? `true` : `false`} onClick={() => toggle(p.index)}>
        <span {...part.with(`chevron`, { open: isOpen() })}>
          <IconGlyph icons={ctx.host.icons} name="ui-chevron-right" />
        </span>
        <StatusGlyph part={part.with(`sectionIcon`, { status: s().status })} status={s().status} />
        <span {...part.with(`sectionTitle`, { status: s().status })}>{s().title}</span>
        <Hidden>{ctx.t(ciStatusEntry(s().status).label)}</Hidden>
        <Show when={s().duration}>
          <span {...part(`sectionMeta`)}>{s().duration}</span>
        </Show>
      </button>
    )
  }
  const LineRow = (p: { n: number }) => {
    const line = () => model().lines[p.n - 1]!
    const segments = createMemo(() => {
      const marks = byLine().get(p.n)
      return ciSegments(tokensOf(p.n), marks?.list ?? [], marks?.first ?? 0, current())
    })
    const kind = () => line().kind
    const tok = (t: CiToken) => <span {...part.with(`token`, { kind: kind() ?? t.kind })}>{t.text}</span>
    const selected = () => {
      const r = range()
      return r !== null && p.n >= r.start && p.n <= r.end
    }
    return (
      <div {...part.atWith(`line`, p.n, { kind: kind() ?? `plain` }, selected() && `selected`)}>
        <Show when={line().section >= 0}>
          <span class="xui-log-indent" />
        </Show>
        <a
          {...part.at(`gutter`, p.n)}
          href={`#L${p.n}`}
          aria-label={ctx.t(`lineAnchor`, { line: p.n })}
          style={css({ minWidth: gutterWidth() })}
          onClick={(e) => {
            e.preventDefault()
            press(p.n, e.shiftKey)
          }}
        >
          {p.n}
        </a>
        <Show when={timestamps()}>
          <span {...part.at(`time`, p.n)}>{line().time ? line().time : null}</span>
        </Show>
        <span {...part.at(`code`, p.n)}>
          <For each={segments()}>{(seg) => (seg.mark === null ? <For each={seg.tokens}>{tok}</For> : <span {...part.at(`match`, seg.index, seg.mark === `current` && `selected`)}>{<For each={seg.tokens}>{tok}</For>}</span>)}</For>
        </span>
      </div>
    )
  }

  const rowsStyle = () => css({ "--xui-log-cols": cols(), ...(view().width ? { "--xui-log-width": `${view().width}px` } : {}) })
  return (
    <div {...props.rootProps}>
      <div {...part(`toolbar`)}>
        <span class="xui-log-find">
        <input
          type="search"
          {...part(`search`)}
          value={query()}
          attr:value={query()}
          placeholder={ctx.t(`searchLog`)}
          aria-label={ctx.t(`searchLog`)}
          autocomplete="off"
          onInput={(e) => search(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key !== `Enter`) return
            e.preventDefault()
            step(e.shiftKey ? -1 : 1)
          }}
        />
        <Show when={query()}>
          <span {...part(`count`)}>{ctx.t(`matchCount`, { current: ctx.formatter.number(current() + 1), total: ctx.formatter.number(matches().length) })}</span>
        </Show>
        <button type="button" {...part(`prev`, !matches().length && `disabled`)} aria-label={ctx.t(`previousMatch`)} disabled={!matches().length || undefined} onClick={() => step(-1)}>
          <IconGlyph icons={ctx.host.icons} name="ui-chevron-up" />
        </button>
        <button type="button" {...part(`next`, !matches().length && `disabled`)} aria-label={ctx.t(`nextMatch`)} disabled={!matches().length || undefined} onClick={() => step(1)}>
          <IconGlyph icons={ctx.host.icons} name="ui-chevron-down" />
        </button>
        </span>
        <span class="xui-log-toggles">
        <button type="button" {...part(`follow`, follow() && `selected`)} aria-pressed={follow() ? `true` : `false`} onClick={() => turnFollow(!follow())}>
          {ctx.t(`follow`)}
        </button>
        <button
          type="button"
          {...part(`timestamps`, timestamps() && `selected`)}
          aria-pressed={timestamps() ? `true` : `false`}
          onClick={() => {
            const next = !timestamps()
            setTimestamps(next)
            void props.emit(`timestamps`, { timestamps: next }, { timestamps: next })
          }}
        >
          {ctx.t(`timestamps`)}
        </button>
        </span>
      </div>
      <div ref={body} {...part(`body`)} role="log" aria-live="polite" tabindex={0} style={css({ "--xui-log-lines": maxLines() })} onScroll={onScroll}>
        <div class="xui-log-rows" style={rowsStyle()}>
          <Show when={win().before > 0}>
            <div class="xui-log-gap" style={css({ height: win().before })} />
          </Show>
          <For each={keys()}>{(key) => (key[0] === `s` ? <SectionRow index={Number(key.slice(1))} /> : <LineRow n={Number(key.slice(1))} />)}</For>
          <Show when={win().after > 0}>
            <div class="xui-log-gap" style={css({ height: win().after })} />
          </Show>
        </div>
      </div>
    </div>
  )
}

export function JobGraphNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const compact = () => ciCompact(ctx.breakpoint, props.tokens.breakpoint as Record<string, number> | undefined)
  const layout = createMemo(() => jobGraphLayout(arr<unknown>(props.props.jobs), compact()))
  const [value, setValue] = createBoundState<string>(
    () => props.node,
    () => props.scope,
    `value`,
    () => str(props.props.value)
  )
  const [hovered, setHovered] = createSignal<string | null>(null)
  const [focused, setFocused] = createSignal<string | null>(null)
  const upstream = createMemo(() => {
    const active = hovered() ?? focused()
    return new Set(active ? jobGraphUpstream(layout(), active) : [])
  })
  const select = (id: string) => {
    setValue(id)
    void props.emit(`select`, { id }, { value: id })
  }
  const label = (n: JobGraphNode) => {
    const out = [n.name, ctx.t(ciStatusEntry(n.status).label)]
    if (n.count) out.push(`${n.count.passed}/${n.count.total}`)
    return out.join(`, `)
  }
  const meta = (n: JobGraphNode) => (n.count ? `${n.count.passed}/${n.count.total}` : n.duration)
  const inner = (n: () => JobGraphNode, i: () => number) => (
    <>
      <StatusGlyph part={part.atWith(`icon`, i(), { status: n().status })} status={n().status} />
      <span {...part.atWith(`name`, i(), { status: n().status })}>{n().name}</span>
      <Show when={meta(n())}>
        <span {...part.at(`meta`, i())}>{meta(n())}</span>
      </Show>
    </>
  )
  // Nodes keyed by id (React's `key`), read live by their position.
  const ids = createMemo(
    () => layout().nodes.map((n) => n.id),
    undefined,
    { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) }
  )
  const nodeOf = (id: string) => {
    const i = createMemo(() => ids().indexOf(id))
    return { i, n: () => layout().nodes[i()]! }
  }
  return (
    <Show
      when={layout().mode === `graph`}
      fallback={
        <div {...props.rootProps} role="group" aria-label={ctx.t(`jobGraph`)}>
          <For each={ids()}>
            {(id) => {
              const { i, n } = nodeOf(id)
              return (
                <button type="button" {...part.atWith(`row`, i(), { status: n().status }, value() === id && `selected`)} aria-label={label(n())} aria-current={value() === id ? `true` : undefined} onClick={() => select(id)}>
                  {inner(n, i)}
                  <span {...part.at(`chevron`, i())}>
                    <IconGlyph icons={ctx.host.icons} name="ui-chevron-right" />
                  </span>
                </button>
              )
            }}
          </For>
        </div>
      }
    >
      <div {...props.rootProps} role="group" aria-label={ctx.t(`jobGraph`)}>
        <div class="xui-jobgraph-canvas" style={css({ width: layout().width, height: layout().height })}>
          <For each={layout().stages}>
            {(s, i) => (
              <span {...part.at(`stage`, i())} style={css({ left: s.x, top: 0, width: JOB_GRAPH.nodeWidth, height: JOB_GRAPH.stageHeight })}>
                {s.title}
              </span>
            )}
          </For>
          <svg class="xui-jobgraph-edges" width={layout().width} height={layout().height} aria-hidden="true">
            <For each={layout().edges.filter((e) => upstream().has(e.from) && upstream().has(e.to))}>
              {(e) => <path {...part(`edge`)} stroke-width={JOB_GRAPH.edgeWidth} d={`M${e.start[0]} ${e.start[1]}C${e.c1[0]} ${e.c1[1]} ${e.c2[0]} ${e.c2[1]} ${e.end[0]} ${e.end[1]}`} />}
            </For>
          </svg>
          <For each={ids()}>
            {(id) => {
              const { i, n } = nodeOf(id)
              return (
                <button
                  type="button"
                  {...part.atWith(`node`, i(), { status: n().status }, upstream().has(id) && `highlight`, value() === id && `selected`)}
                  aria-label={label(n())}
                  aria-current={value() === id ? `true` : undefined}
                  style={css({ left: n().x, top: n().y, width: n().width, height: n().height })}
                  onClick={() => select(id)}
                  onMouseEnter={() => setHovered(id)}
                  onMouseLeave={() => setHovered((h) => (h === id ? null : h))}
                  onFocus={() => setFocused(id)}
                  onBlur={() => setFocused((f) => (f === id ? null : f))}
                >
                  {inner(n, i)}
                </button>
              )
            }}
          </For>
        </div>
      </div>
    </Show>
  )
}

export const NATIVES_CI: Record<string, Component<NativeProps>> = {
  LogLines: LogLinesNative,
  JobGraph: JobGraphNative,
}
