// #EXP-2204 (docs/ci-components-contract.md §1–§5, a11y.json LogLines +
// JobGraph): the CI pieces. LogLines = LogView's toolbar (search, `n of m`,
// prev/next, Follow, Timestamps) over the WINDOWED log (fixed-height rows
// of `logRows`: folding step rows, numbered lines with pressable anchors,
// the ansi tokens, the search marks); JobGraph = the stages as columns
// (`jobGraphLayout`), the needs of the hovered or focused node as cubic
// connectors, a status list below `md`. The rules are the core's
// (src/ci-log.ts, src/job-graph.ts), the painting helpers web/ci.ts, so
// the Solid twin (natives/ci.tsx there) builds the same DOM.

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent } from "react"
import { JOB_GRAPH, ciStatusEntry, extendLineAnchor, jobGraphLayout, jobGraphUpstream, logModel, logOpenDefault, logReveal, logRows, logSearch, parseLineAnchor, resolveRecipe, tokenizeCode, type LogMatch } from "@exponential_app/eui"
import { CI_FOLLOW_SLACK, LOG_VIEW, ciBottom, ciColumns, ciCompact, ciOffsets, ciRevealOffset, ciSegments, ciWindow, type CiToken } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { arr, bool, Hidden, num, str, useParts, type Attrs } from "./shared"

const A = (a: Attrs) => a as Record<string, string>

/** A recipe length of the theme (a number), else `fallback`. */
function recipeLength(theme: NativeProps[`theme`], mode: NativeProps[`mode`], component: string, part: string, key: string, fallback: number): number {
  const v = resolveRecipe(theme, { component, part }, mode)[key]
  return typeof v === `number` ? v : fallback
}

const numbers = (v: unknown): number[] => (Array.isArray(v) ? v.filter((x): x is number => typeof x === `number` && Number.isInteger(x)) : [])

/** The status glyph (§1): the table's icon, spinning while running. */
function StatusGlyph({ part, status }: { part: Attrs; status: unknown }) {
  const ctx = useSurfaceContext()
  const entry = ciStatusEntry(status)
  return (
    <span {...A(part)} data-xui-ci-spin={entry.spin ? `` : undefined}>
      <IconGlyph icons={ctx.host.icons} name={entry.icon} />
    </span>
  )
}

export function LogLinesNative({ node, props, rootProps, emit, scope, theme, mode }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const model = useMemo(() => logModel(arr<unknown>(props.lines), arr<unknown>(props.sections)), [props.lines, props.sections])
  const fallbackOpen = useMemo(() => logOpenDefault(model), [model])
  const [open, setOpen] = useBoundState<number[]>(node, scope, `open`, Array.isArray(props.open) ? numbers(props.open) : fallbackOpen)
  const [query, setQuery] = useBoundState<string>(node, scope, `query`, str(props.query))
  const [selection, setSelection] = useBoundState<string>(node, scope, `selection`, str(props.selection))
  const [follow, setFollow] = useBoundState<boolean>(node, scope, `follow`, props.follow === undefined || props.follow === null ? true : bool(props.follow))
  const [timestamps, setTimestamps] = useBoundState<boolean>(node, scope, `timestamps`, bool(props.timestamps))
  const maxLines = props.maxLines === undefined || props.maxLines === null ? LOG_VIEW.maxLines : Math.max(1, num(props.maxLines))

  // Geometry: the theme's line height and step row height (fixed rows).
  const lineH = recipeLength(theme, mode, `LogLines`, `line`, `lineHeight`, 22.75)
  const sectionH = recipeLength(theme, mode, `LogLines`, `section`, `height`, 40)
  const pad = recipeLength(theme, mode, `LogLines`, `body`, `paddingVertical`, 0)
  const rows = useMemo(() => logRows(model, open), [model, open])
  const offsets = useMemo(() => ciOffsets(rows.map((r) => (r.kind === `section` ? sectionH : lineH))), [rows, sectionH, lineH])
  const total = offsets[rows.length] ?? 0
  const cols = useMemo(() => ciColumns(model.lines.map((l) => l.text)), [model])
  const gutterWidth = `${String(model.lines.length).length + 1}ch`

  // Search.
  const matches = useMemo(() => logSearch(model, query), [model, query])
  const byLine = useMemo(() => {
    const map = new Map<number, { first: number; list: LogMatch[] }>()
    matches.forEach((m, i) => {
      const hit = map.get(m.line)
      if (hit) hit.list.push(m)
      else map.set(m.line, { first: i, list: [m] })
    })
    return map
  }, [matches])
  const [currentRaw, setCurrent] = useState(0)
  const current = matches.length ? Math.min(currentRaw, matches.length - 1) : -1
  const range = parseLineAnchor(selection)

  // The ansi tokens, per line, built when a line first shows.
  const tokens = useMemo(() => new Map<number, CiToken[]>(), [model])
  const tokensOf = (n: number): CiToken[] => {
    let hit = tokens.get(n)
    if (!hit) {
      hit = (tokenizeCode(model.lines[n - 1]?.text ?? ``, `ansi`)[0] ?? []) as CiToken[]
      tokens.set(n, hit)
    }
    return hit
  }

  // The window: the body's scroll and size (content coordinates).
  const body = useRef<HTMLDivElement | null>(null)
  const initialViewport = maxLines * lineH + 2 * pad
  const [view, setView] = useState<{ scroll: number; viewport: number; width: number }>({ scroll: 0, viewport: initialViewport, width: 0 })
  const viewport = view.viewport || initialViewport
  const win = ciWindow(rows.map((r) => (r.kind === `section` ? sectionH : lineH)), offsets, view.scroll - pad, viewport)
  const lastTop = useRef(0)
  const ignoreTop = useRef<number | null>(null)
  const scrollTo = (top: number) => {
    const el = body.current
    if (el) el.scrollTop = top
    ignoreTop.current = top
    lastTop.current = top
    setView((v) => (v.scroll === top ? v : { ...v, scroll: top }))
  }
  const turnFollow = (next: boolean) => {
    setFollow(next)
    void emit(`follow`, { follow: next }, { follow: next })
  }
  const onScroll = () => {
    const el = body.current
    if (!el) return
    const top = el.scrollTop
    const programmatic = ignoreTop.current !== null && Math.abs(top - ignoreTop.current) < 1
    ignoreTop.current = null
    if (!programmatic && follow && top < lastTop.current - 1 && el.scrollHeight - top - el.clientHeight > CI_FOLLOW_SLACK) turnFollow(false)
    lastTop.current = top
    setView((v) => (v.scroll === top ? v : { ...v, scroll: top }))
  }
  useLayoutEffect(() => {
    const el = body.current
    if (!el || typeof ResizeObserver === `undefined`) return
    const read = () => setView((v) => (v.viewport === (el.clientHeight || v.viewport) && v.width === el.clientWidth ? v : { ...v, viewport: el.clientHeight || v.viewport, width: el.clientWidth }))
    read()
    const ro = new ResizeObserver(read)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  // Follow: stick to the bottom as lines arrive (and when switched on).
  useLayoutEffect(() => {
    if (follow) scrollTo(ciBottom(total + 2 * pad, viewport))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [follow, total, viewport])

  // Reveal: open the line's section, then scroll it in (top third).
  const pending = useRef<{ line: number; always: boolean } | null>(null)
  const [revealTick, setRevealTick] = useState(0)
  const reveal = (line: number, always: boolean) => {
    const next = logReveal(model, open, line)
    if (next.length !== open.length || next.some((v, i) => v !== open[i])) setOpen(next)
    pending.current = { line, always }
    setRevealTick((t) => t + 1)
  }
  useLayoutEffect(() => {
    const p = pending.current
    if (!p) return
    const at = rows.findIndex((r) => r.kind === `line` && r.n === p.line)
    if (at < 0) return
    pending.current = null
    scrollTo(ciRevealOffset((offsets[at] ?? 0) + pad, lineH, view.scroll, viewport, total + 2 * pad, p.always))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, revealTick])
  // A selection that is new from outside (or the first) reveals its start
  // and stops the follow; a press on a number does neither.
  // (A passive effect: it runs after useBoundState's own first sync, which
  // would otherwise undo a write made while mounting.)
  const own = useRef<string | null>(null)
  useEffect(() => {
    if (selection === own.current) return
    own.current = selection
    const r = parseLineAnchor(selection)
    if (!r) return
    reveal(r.start, true)
    if (follow) turnFollow(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection])

  const toggle = (index: number) => {
    const was = open.includes(index)
    const next = was ? open.filter((v) => v !== index) : [...open, index].sort((a, b) => a - b)
    setOpen(next)
    void emit(`toggle`, { section: index, open: !was }, { open: next })
  }
  const search = (q: string) => {
    setQuery(q)
    setCurrent(0)
    void emit(`search`, { query: q }, { query: q })
    const first = logSearch(model, q)[0]
    if (first) reveal(first.line, false)
  }
  const step = (by: number) => {
    if (!matches.length) return
    const next = (((current < 0 ? 0 : current) + by) % matches.length + matches.length) % matches.length
    setCurrent(next)
    reveal(matches[next]!.line, false)
  }
  const press = (n: number, shift: boolean) => {
    const anchor = extendLineAnchor(selection, n, shift)
    const r = parseLineAnchor(anchor)!
    own.current = anchor
    setSelection(anchor)
    void emit(`select`, { anchor, start: r.start, end: r.end }, { selection: anchor })
    if (follow) turnFollow(false)
  }
  const onGutter = (n: number) => (e: ReactMouseEvent) => {
    e.preventDefault()
    press(n, e.shiftKey)
  }

  const seen: React.ReactNode[] = []
  for (let i = win.start; i < win.end; i++) {
    const row = rows[i]!
    if (row.kind === `section`) {
      const s = model.sections[row.section]!
      const isOpen = open.includes(s.index)
      const entry = ciStatusEntry(s.status)
      seen.push(
        <button key={`s${s.index}`} type="button" {...A(part.atWith(`section`, s.index, { status: s.status, open: isOpen }))} aria-expanded={isOpen ? `true` : `false`} onClick={() => toggle(s.index)}>
          <span {...A(part.with(`chevron`, { open: isOpen }))}>
            <IconGlyph icons={ctx.host.icons} name="ui-chevron-right" />
          </span>
          <StatusGlyph part={part.with(`sectionIcon`, { status: s.status })} status={s.status} />
          <span {...A(part.with(`sectionTitle`, { status: s.status }))}>{s.title}</span>
          <Hidden>{ctx.t(entry.label)}</Hidden>
          {s.duration ? <span {...A(part(`sectionMeta`))}>{s.duration}</span> : null}
        </button>
      )
      continue
    }
    const line = model.lines[row.n - 1]!
    const n = line.n
    const marks = byLine.get(n)
    const segments = ciSegments(tokensOf(n), marks?.list ?? [], marks?.first ?? 0, current)
    const kind = line.kind
    const tok = (t: CiToken, j: number) => (
      <span key={j} {...A(part.with(`token`, { kind: kind ?? t.kind }))}>
        {t.text}
      </span>
    )
    seen.push(
      <div key={`l${n}`} {...A(part.atWith(`line`, n, { kind: kind ?? `plain` }, range !== null && n >= range.start && n <= range.end && `selected`))}>
        {line.section >= 0 ? <span className="xui-log-indent" /> : null}
        <a {...A(part.at(`gutter`, n))} href={`#L${n}`} aria-label={ctx.t(`lineAnchor`, { line: n })} style={{ minWidth: gutterWidth }} onClick={onGutter(n)}>
          {n}
        </a>
        {timestamps ? <span {...A(part.at(`time`, n))}>{line.time ? line.time : null}</span> : null}
        <span {...A(part.at(`code`, n))}>{segments.map((seg, k) => (seg.mark === null ? seg.tokens.map((t, j) => tok(t, k * 1000 + j)) : <span key={`m${seg.index}`} {...A(part.at(`match`, seg.index, seg.mark === `current` && `selected`))}>{seg.tokens.map(tok)}</span>))}</span>
      </div>
    )
  }

  const rowsStyle = { "--xui-log-cols": cols, ...(view.width ? { "--xui-log-width": `${view.width}px` } : {}) } as CSSProperties
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <div {...A(part(`toolbar`))}>
        <span className="xui-log-find">
        <input
          type="search"
          {...A(part(`search`))}
          value={query}
          placeholder={ctx.t(`searchLog`)}
          aria-label={ctx.t(`searchLog`)}
          autoComplete="off"
          onChange={(e) => search(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key !== `Enter`) return
            e.preventDefault()
            step(e.shiftKey ? -1 : 1)
          }}
        />
        {query ? <span {...A(part(`count`))}>{ctx.t(`matchCount`, { current: ctx.formatter.number(current + 1), total: ctx.formatter.number(matches.length) })}</span> : null}
        <button type="button" {...A(part(`prev`, !matches.length && `disabled`))} aria-label={ctx.t(`previousMatch`)} disabled={!matches.length || undefined} onClick={() => step(-1)}>
          <IconGlyph icons={ctx.host.icons} name="ui-chevron-up" />
        </button>
        <button type="button" {...A(part(`next`, !matches.length && `disabled`))} aria-label={ctx.t(`nextMatch`)} disabled={!matches.length || undefined} onClick={() => step(1)}>
          <IconGlyph icons={ctx.host.icons} name="ui-chevron-down" />
        </button>
        </span>
        <span className="xui-log-toggles">
        <button type="button" {...A(part(`follow`, follow && `selected`))} aria-pressed={follow ? `true` : `false`} onClick={() => turnFollow(!follow)}>
          {ctx.t(`follow`)}
        </button>
        <button
          type="button"
          {...A(part(`timestamps`, timestamps && `selected`))}
          aria-pressed={timestamps ? `true` : `false`}
          onClick={() => {
            const next = !timestamps
            setTimestamps(next)
            void emit(`timestamps`, { timestamps: next }, { timestamps: next })
          }}
        >
          {ctx.t(`timestamps`)}
        </button>
        </span>
      </div>
      <div ref={body} {...A(part(`body`))} role="log" aria-live="polite" tabIndex={0} style={{ "--xui-log-lines": maxLines } as CSSProperties} onScroll={onScroll}>
        <div className="xui-log-rows" style={rowsStyle}>
          {win.before > 0 ? <div className="xui-log-gap" style={{ height: win.before }} /> : null}
          {seen}
          {win.after > 0 ? <div className="xui-log-gap" style={{ height: win.after }} /> : null}
        </div>
      </div>
    </div>
  )
}

export function JobGraphNative({ node, props, rootProps, emit, scope, tokens }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const compact = ciCompact(ctx.breakpoint, tokens.breakpoint as Record<string, number> | undefined)
  const layout = useMemo(() => jobGraphLayout(arr<unknown>(props.jobs), compact), [props.jobs, compact])
  const [value, setValue] = useBoundState<string>(node, scope, `value`, str(props.value))
  const [hovered, setHovered] = useState<string | null>(null)
  const [focused, setFocused] = useState<string | null>(null)
  const active = hovered ?? focused
  const upstream = useMemo(() => new Set(active ? jobGraphUpstream(layout, active) : []), [layout, active])
  const select = (id: string) => {
    setValue(id)
    void emit(`select`, { id }, { value: id })
  }
  const label = (n: (typeof layout.nodes)[number]) => {
    const parts = [n.name, ctx.t(ciStatusEntry(n.status).label)]
    if (n.count) parts.push(`${n.count.passed}/${n.count.total}`)
    return parts.join(`, `)
  }
  const meta = (n: (typeof layout.nodes)[number]) => (n.count ? `${n.count.passed}/${n.count.total}` : n.duration)
  const inner = (n: (typeof layout.nodes)[number], i: number) => (
    <>
      <StatusGlyph part={part.atWith(`icon`, i, { status: n.status })} status={n.status} />
      <span {...A(part.atWith(`name`, i, { status: n.status }))}>{n.name}</span>
      {meta(n) ? <span {...A(part.at(`meta`, i))}>{meta(n)}</span> : null}
    </>
  )
  if (layout.mode === `list`) {
    return (
      <div {...(rootProps as Record<string, unknown>)} role="group" aria-label={ctx.t(`jobGraph`)}>
        {layout.nodes.map((n, i) => (
          <button key={n.id} type="button" {...A(part.atWith(`row`, i, { status: n.status }, value === n.id && `selected`))} aria-label={label(n)} aria-current={value === n.id ? `true` : undefined} onClick={() => select(n.id)}>
            {inner(n, i)}
            <span {...A(part.at(`chevron`, i))}>
              <IconGlyph icons={ctx.host.icons} name="ui-chevron-right" />
            </span>
          </button>
        ))}
      </div>
    )
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} role="group" aria-label={ctx.t(`jobGraph`)}>
      <div className="xui-jobgraph-canvas" style={{ width: layout.width, height: layout.height }}>
        {layout.stages.map((s, i) => (
          <span key={i} {...A(part.at(`stage`, i))} style={{ left: s.x, top: 0, width: JOB_GRAPH.nodeWidth, height: JOB_GRAPH.stageHeight }}>
            {s.title}
          </span>
        ))}
        <svg className="xui-jobgraph-edges" width={layout.width} height={layout.height} aria-hidden="true">
          {layout.edges
            .filter((e) => upstream.has(e.from) && upstream.has(e.to))
            .map((e) => (
              <path key={`${e.from}>${e.to}`} {...A(part(`edge`))} strokeWidth={JOB_GRAPH.edgeWidth} d={`M${e.start[0]} ${e.start[1]}C${e.c1[0]} ${e.c1[1]} ${e.c2[0]} ${e.c2[1]} ${e.end[0]} ${e.end[1]}`} />
            ))}
        </svg>
        {layout.nodes.map((n, i) => (
          <button
            key={n.id}
            type="button"
            {...A(part.atWith(`node`, i, { status: n.status }, upstream.has(n.id) && `highlight`, value === n.id && `selected`))}
            aria-label={label(n)}
            aria-current={value === n.id ? `true` : undefined}
            style={{ left: n.x, top: n.y, width: n.width, height: n.height }}
            onClick={() => select(n.id)}
            onMouseEnter={() => setHovered(n.id)}
            onMouseLeave={() => setHovered((h) => (h === n.id ? null : h))}
            onFocus={() => setFocused(n.id)}
            onBlur={() => setFocused((f) => (f === n.id ? null : f))}
          >
            {inner(n, i)}
          </button>
        ))}
      </div>
    </div>
  )
}
