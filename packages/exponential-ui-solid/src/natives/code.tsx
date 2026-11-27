// FEED-128 (React's natives/code.tsx; round 1 contract §3 CodeBlock,
// a11y.json CodeBlock): the code block on the catalog's ONE tokenizer
// (`tokenizeCode`, catalog/code.json), painted per token kind through the
// `CodeBlock/token` recipe (`when: {kind}`). An optional header (title +
// the copy button: `builtinIcons` copy/copied, `copied` announced), a
// line-number gutter, emphasised `highlight` lines (the `selected` state),
// `wrap`, and `maxLines` before the block scrolls. EXP-1324: `variant`
// (destructive) and `size` (sm) are recipe props; `ansi` output copies
// without its escapes. #EXP-1804: `follow` keeps a scrolling block at its
// bottom while `code` grows (a live log) until the person scrolls up; back
// at the bottom, it follows again. #EXP-2204 (docs/ci-components-contract.md
// §6): `anchors`, the `selection` shading, `gutter: blame`, the window of a
// self-scrolling block past `windowThreshold` lines (React's, element for
// element).

import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, untrack, type Component, type JSX } from "solid-js"
import { blameBlocks, codeText, extendLineAnchor, parseLineAnchor, resolveRecipe, tokenizeCode, WINDOW_THRESHOLD } from "@exponential_app/eui"
import { CI_FOLLOW_SLACK as FOLLOW_SLACK, LOG_VIEW, ciOffsets, ciRevealOffset, ciSized, ciWindow } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { arr, bool, BuiltinIcon, css, num, objects, parts, str } from "./shared"

export function CodeBlockNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const code = () => str(props.props.code)
  const language = () => str(props.props.language, `plain`)
  const lines = createMemo(() => tokenizeCode(code(), language()))
  const highlight = createMemo(() => new Set(arr<unknown>(props.props.highlight).map((v) => num(v))))
  const anchors = () => bool(props.props.anchors)
  const numbers = () => bool(props.props.lineNumbers) || anchors()
  const copyable = () => props.props.copyable !== false
  const title = () => str(props.props.title)
  // Round 2 (§7 CodeBlock): an untitled block names its language in the
  // header (`ts`; none for `plain`); the region's name stays `codeBlock`.
  const heading = () => title() || (language() === `plain` ? `` : language())
  const maxLines = () => (props.props.maxLines === undefined ? 0 : num(props.props.maxLines))
  const follow = () => bool(props.props.follow)
  const wrap = () => bool(props.props.wrap)
  const [selection, setSelection] = createBoundState<string>(
    () => props.node,
    () => props.scope,
    `selection`,
    () => str(props.props.selection)
  )
  const range = createMemo(() => parseLineAnchor(selection()))
  const blameOn = () => props.props.gutter === `blame`
  const blame = () => objects<{ author?: unknown; age?: unknown }>(props.props.blame)
  const blameLines = createMemo(() => (blameOn() ? blameBlocks(arr<unknown>(props.props.blame), lines().length) : null))
  // The window (uniform lines: `wrap` off only) of a block that scrolls itself.
  const lineH = () => {
    const v = resolveRecipe(props.theme, { component: `CodeBlock`, part: `root`, props: { size: props.props.size } }, props.mode).lineHeight
    return typeof v === `number` ? v : 22.75
  }
  const windowed = () => !wrap() && (maxLines() > 0 || ciSized(props.node.style)) && lines().length > WINDOW_THRESHOLD
  const initialViewport = () => (maxLines() > 0 ? maxLines() : LOG_VIEW.maxLines) * lineH()
  const [view, setView] = createSignal({ scroll: 0, viewport: untrack(initialViewport) })
  const extents = createMemo(() => (windowed() ? lines().map(() => lineH()) : []))
  const offsets = createMemo(() => ciOffsets(extents()))
  const win = createMemo(() => (windowed() ? ciWindow(extents(), offsets(), view().scroll, view().viewport || initialViewport()) : { start: 0, end: lines().length, before: 0, after: 0, total: 0 }), undefined, {
    equals: (a, b) => a.start === b.start && a.end === b.end && a.before === b.before && a.after === b.after,
  })
  const indices = createMemo(
    () => {
      const w = win()
      const out: number[] = []
      for (let i = w.start; i < w.end; i++) out.push(i)
      return out
    },
    undefined,
    { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) }
  )
  let body: HTMLDivElement | undefined
  // Whether the view sat at its bottom at the last scroll (a person who
  // scrolled up stops the follow).
  let atBottom = true
  const onScroll = () => {
    if (!body) return
    if (follow()) atBottom = body.scrollHeight - body.scrollTop - body.clientHeight <= FOLLOW_SLACK
    const el = body
    if (windowed()) setView((v) => (v.scroll === el.scrollTop && v.viewport === (el.clientHeight || v.viewport) ? v : { scroll: el.scrollTop, viewport: el.clientHeight || v.viewport }))
  }
  // After the lines render (the effect runs after the DOM update).
  createEffect(
    on([follow, code, maxLines], ([following]) => {
      if (!following || !body || !atBottom) return
      body.scrollTop = body.scrollHeight
      if (untrack(windowed)) setView((v) => ({ ...v, scroll: Math.max(0, untrack(lines).length * untrack(lineH) - (v.viewport || untrack(initialViewport))) }))
    }),
  )
  // A selection that is new from outside (or the first) scrolls its first
  // line into a self-scrolling body; a press on a number does not.
  let own: string | null = null
  createEffect(
    on(selection, (sel) => {
      if (sel === own) return
      own = sel
      const r = parseLineAnchor(sel)
      const el = body
      untrack(() => {
        if (!r || !el || !(maxLines() > 0 || windowed())) return
        const viewport = el.clientHeight || view().viewport || initialViewport()
        const top = ciRevealOffset((r.start - 1) * lineH(), lineH(), el.scrollTop, viewport, lines().length * lineH(), false)
        el.scrollTop = top
        if (windowed()) setView((v) => ({ ...v, scroll: top }))
      })
    })
  )
  const press = (n: number, e: MouseEvent) => {
    e.preventDefault()
    const anchor = extendLineAnchor(selection(), n, e.shiftKey)
    const r = parseLineAnchor(anchor)!
    own = anchor
    setSelection(anchor)
    void props.emit(`select`, { anchor, start: r.start, end: r.end }, { selection: anchor })
  }
  const [copied, setCopied] = createSignal(false)
  let timer: ReturnType<typeof setTimeout> | null = null
  onCleanup(() => {
    if (timer) clearTimeout(timer)
  })
  const copy = async () => {
    // Acknowledge only a copy that happened: no clipboard (an insecure
    // origin) or a refusal leaves the button as it was and says nothing.
    const clipboard = typeof navigator === `undefined` ? undefined : navigator.clipboard
    if (!clipboard?.writeText) return
    try {
      await clipboard.writeText(codeText(code(), language()))
    } catch {
      return
    }
    setCopied(true)
    ctx.announce(ctx.t(`copied`))
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => setCopied(false), 1500)
  }
  const gutterWidth = () => `${String(lines().length).length + 1}ch`
  const bodyClass = () => `${part(`body`).class as string} xui-codeblock-scroll`
  const codeClass = () => `${part(`code`).class as string} xui-codeblock-text`
  return (
    <div {...props.rootProps} role="region" aria-label={title() || ctx.t(`codeBlock`)} data-language={language()}>
      <Show when={heading() || copyable()}>
        <div {...part(`header`)}>
          <Show when={heading()} fallback={<span class="xui-codeblock-spacer" />}>
            <span {...part(`title`)}>{heading()}</span>
          </Show>
          <Show when={copyable()}>
            <button type="button" {...part(`copy`, copied() && `checked`)} aria-label={copied() ? ctx.t(`copied`) : ctx.t(`copy`)} onClick={() => void copy()}>
              <BuiltinIcon slot={copied() ? `CodeBlock.copied` : `CodeBlock.copy`} />
            </button>
          </Show>
        </div>
      </Show>
      <div ref={body} onScroll={onScroll} {...part(`body`)} class={bodyClass()} data-wrap={wrap() ? `true` : undefined} style={maxLines() > 0 ? css({ "--xui-code-lines": maxLines() }) : undefined} data-max-lines={maxLines() > 0 ? maxLines() : undefined} tabindex={0}>
        <pre class="xui-codeblock-pre" id={`${props.domId}.code`}>
          <code>
            <Show when={win().before > 0}>
              <span class="xui-codeblock-gap" style={css({ height: win().before })} aria-hidden="true" />
            </Show>
            <For each={indices()}>
              {(i) => {
                const n = i + 1
                const tokens = () => lines()[i] ?? []
                const selected = () => {
                  const r = range()
                  return highlight().has(n) || (r !== null && n >= r.start && n <= r.end)
                }
                const b = () => blameLines()?.[i]
                const who = () => {
                  const x = b()
                  return x && x.first && x.block >= 0 ? blame()[x.block] : undefined
                }
                return (
                  <span {...part.at(`line`, i, selected() && `selected`)} data-line={n}>
                    <Show when={b()}>
                      <span {...part.at(`blame`, i)} data-border={b()!.border ? `` : undefined}>
                        <Show when={who()}>
                          <span {...part(`blameAuthor`)}>{str(who()!.author)}</span>
                          <span {...part(`blameAge`)}>{str(who()!.age)}</span>
                        </Show>
                      </span>
                    </Show>
                    <Show when={numbers()}>
                      <Show
                        when={anchors()}
                        fallback={
                          <span {...part(`gutter`)} style={{ "min-width": gutterWidth() }} aria-hidden="true">
                            <span {...part.at(`lineNumber`, i)}>{n}</span>
                          </span>
                        }
                      >
                        <span {...part(`gutter`)} style={{ "min-width": gutterWidth() }}>
                          <a {...part.at(`lineNumber`, i)} href={`#L${n}`} aria-label={ctx.t(`lineAnchor`, { line: n })} onClick={(e) => press(n, e)}>
                            {n}
                          </a>
                        </span>
                      </Show>
                    </Show>
                    <span {...part.at(`code`, i)} class={codeClass()}>
                      {tokens().length === 0 ? `​` : null}
                      <For each={tokens()}>{(t) => <span {...part.with(`token`, { kind: t.kind })}>{t.text}</span>}</For>
                    </span>
                    {`\n`}
                  </span>
                )
              }}
            </For>
            <Show when={win().after > 0}>
              <span class="xui-codeblock-gap" style={css({ height: win().after })} aria-hidden="true" />
            </Show>
          </code>
        </pre>
      </div>
    </div>
  )
}

export const NATIVES_CODE: Record<string, Component<NativeProps>> = {
  CodeBlock: CodeBlockNative,
}
