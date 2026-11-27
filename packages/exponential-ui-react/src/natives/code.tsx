// Round 1 (contract §3 CodeBlock, a11y.json CodeBlock): the code block on
// the catalog's ONE tokenizer (`tokenizeCode`, catalog/code.json — the same
// tokens on every platform), painted per token kind through the
// `CodeBlock/token` recipe (`when: {kind}`). An optional header (title +
// the copy button: `builtinIcons` copy/copied, `copied` announced), a
// line-number gutter, emphasised `highlight` lines (the `selected` state),
// `wrap`, and `maxLines` before the block scrolls. EXP-1324: `variant`
// (destructive) and `size` (sm) are recipe props (shadcn's code-block look
// lives in the theme); `ansi` output copies without its escapes.
// #EXP-1804: `follow` keeps a scrolling block at its bottom while `code`
// grows (a live log) until the person scrolls up; back at the bottom, it
// follows again. #EXP-2204 (docs/ci-components-contract.md §6): `anchors`
// (pressable line numbers: `L12`, shift = the range, written to a bound
// `selection` + `select`), the `selection` shading, `gutter: blame` (the
// author column of `blameBlocks`), the window of a self-scrolling block
// past `windowThreshold` lines; the header stays pinned above the body.

import { useLayoutEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react"
import { blameBlocks, codeText, extendLineAnchor, parseLineAnchor, resolveRecipe, tokenizeCode, WINDOW_THRESHOLD } from "@exponential_app/eui"
import { CI_FOLLOW_SLACK as FOLLOW_SLACK, LOG_VIEW, ciOffsets, ciRevealOffset, ciSized, ciWindow } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { arr, bool, BuiltinIcon, num, objects, str, useParts } from "./shared"

export function CodeBlockNative({ node, props, rootProps, domId, emit, scope, theme, mode }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const code = str(props.code)
  const language = str(props.language, `plain`)
  const lines = useMemo(() => tokenizeCode(code, language), [code, language])
  const highlight = new Set(arr<unknown>(props.highlight).map((v) => num(v)))
  const anchors = bool(props.anchors)
  const numbers = bool(props.lineNumbers) || anchors
  const copyable = props.copyable !== false
  const title = str(props.title)
  // Round 2 (§7 CodeBlock): an untitled block names its language in the
  // header (`ts`; none for `plain`); the region's name stays `codeBlock`.
  const heading = title || (language === `plain` ? `` : language)
  const maxLines = props.maxLines === undefined ? 0 : num(props.maxLines)
  const follow = bool(props.follow)
  const wrap = bool(props.wrap)
  const [selection, setSelection] = useBoundState<string>(node, scope, `selection`, str(props.selection))
  const range = parseLineAnchor(selection)
  const blameOn = props.gutter === `blame`
  const blame = objects<{ author?: unknown; age?: unknown }>(props.blame)
  const blameLines = useMemo(() => (blameOn ? blameBlocks(arr<unknown>(props.blame), lines.length) : null), [blameOn, props.blame, lines.length])
  // The window (uniform lines: `wrap` off only) of a block that scrolls itself.
  const lineH = (() => {
    const v = resolveRecipe(theme, { component: `CodeBlock`, part: `root`, props: { size: props.size } }, mode).lineHeight
    return typeof v === `number` ? v : 22.75
  })()
  const windowed = !wrap && (maxLines > 0 || ciSized(node.style)) && lines.length > WINDOW_THRESHOLD
  const initialViewport = (maxLines > 0 ? maxLines : LOG_VIEW.maxLines) * lineH
  const [view, setView] = useState({ scroll: 0, viewport: initialViewport })
  const offsets = useMemo(() => (windowed ? ciOffsets(lines.map(() => lineH)) : []), [windowed, lines, lineH])
  const win = windowed ? ciWindow(lines.map(() => lineH), offsets, view.scroll, view.viewport || initialViewport) : { start: 0, end: lines.length, before: 0, after: 0, total: 0 }
  const body = useRef<HTMLDivElement | null>(null)
  // Whether the view sat at its bottom at the last scroll (a person who
  // scrolled up stops the follow).
  const atBottom = useRef(true)
  const onScroll =
    follow || windowed
      ? () => {
          const el = body.current
          if (!el) return
          if (follow) atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight <= FOLLOW_SLACK
          if (windowed) setView((v) => (v.scroll === el.scrollTop && v.viewport === (el.clientHeight || v.viewport) ? v : { scroll: el.scrollTop, viewport: el.clientHeight || v.viewport }))
        }
      : undefined
  useLayoutEffect(() => {
    const el = body.current
    if (!follow || !el || !atBottom.current) return
    el.scrollTop = el.scrollHeight
    if (windowed) setView((v) => ({ ...v, scroll: Math.max(0, lines.length * lineH - (v.viewport || initialViewport)) }))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [follow, code, maxLines])
  // A selection that is new from outside (or the first) scrolls its first
  // line into a self-scrolling body; a press on a number does not.
  const own = useRef<string | null>(null)
  useLayoutEffect(() => {
    if (selection === own.current) return
    own.current = selection
    const el = body.current
    if (!range || !el || !(maxLines > 0 || windowed)) return
    const viewport = el.clientHeight || view.viewport || initialViewport
    const top = ciRevealOffset((range.start - 1) * lineH, lineH, el.scrollTop, viewport, lines.length * lineH, false)
    el.scrollTop = top
    if (windowed) setView((v) => ({ ...v, scroll: top }))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection])
  const press = (n: number) => (e: ReactMouseEvent) => {
    e.preventDefault()
    const anchor = extendLineAnchor(selection, n, e.shiftKey)
    const r = parseLineAnchor(anchor)!
    own.current = anchor
    setSelection(anchor)
    void emit(`select`, { anchor, start: r.start, end: r.end }, { selection: anchor })
  }
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const copy = async () => {
    // Acknowledge only a copy that happened: no clipboard (an insecure
    // origin) or a refusal leaves the button as it was and says nothing.
    const clipboard = typeof navigator === `undefined` ? undefined : navigator.clipboard
    if (!clipboard?.writeText) return
    try {
      await clipboard.writeText(codeText(code, language))
    } catch {
      return
    }
    setCopied(true)
    ctx.announce(ctx.t(`copied`))
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => setCopied(false), 1500)
  }
  const gutterWidth = `${String(lines.length).length + 1}ch`
  const shown: React.ReactNode[] = []
  for (let i = win.start; i < win.end; i++) {
    const tokens = lines[i]!
    const n = i + 1
    const selected = highlight.has(n) || (range !== null && n >= range.start && n <= range.end)
    const b = blameLines?.[i]
    const who = b && b.first && b.block >= 0 ? blame[b.block] : undefined
    shown.push(
      <span key={i} {...(part.at(`line`, i, selected && `selected`) as Record<string, string>)} data-line={n}>
        {b ? (
          <span {...(part.at(`blame`, i) as Record<string, string>)} data-border={b.border ? `` : undefined}>
            {who ? (
              <>
                <span {...(part(`blameAuthor`) as Record<string, string>)}>{str(who.author)}</span>
                <span {...(part(`blameAge`) as Record<string, string>)}>{str(who.age)}</span>
              </>
            ) : null}
          </span>
        ) : null}
        {numbers ? (
          anchors ? (
            <span {...(part(`gutter`) as Record<string, string>)} style={{ minWidth: gutterWidth }}>
              <a {...(part.at(`lineNumber`, i) as Record<string, string>)} href={`#L${n}`} aria-label={ctx.t(`lineAnchor`, { line: n })} onClick={press(n)}>
                {n}
              </a>
            </span>
          ) : (
            <span {...(part(`gutter`) as Record<string, string>)} style={{ minWidth: gutterWidth }} aria-hidden="true">
              <span {...(part.at(`lineNumber`, i) as Record<string, string>)}>{n}</span>
            </span>
          )
        ) : null}
        <span {...(part.at(`code`, i) as Record<string, string>)} className={`${(part(`code`) as { className: string }).className} xui-codeblock-text`}>
          {tokens.length === 0 ? `​` : null}
          {tokens.map((t, j) => (
            <span key={j} {...(part.with(`token`, { kind: t.kind }) as Record<string, string>)}>
              {t.text}
            </span>
          ))}
        </span>
        {`\n`}
      </span>
    )
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} role="region" aria-label={title || ctx.t(`codeBlock`)} data-language={language}>
      {heading || copyable ? (
        <div {...(part(`header`) as Record<string, string>)}>
          {heading ? <span {...(part(`title`) as Record<string, string>)}>{heading}</span> : <span className="xui-codeblock-spacer" />}
          {copyable ? (
            <button type="button" {...(part(`copy`, copied && `checked`) as Record<string, string>)} aria-label={copied ? ctx.t(`copied`) : ctx.t(`copy`)} onClick={() => void copy()}>
              <BuiltinIcon slot={copied ? `CodeBlock.copied` : `CodeBlock.copy`} />
            </button>
          ) : null}
        </div>
      ) : null}
      <div ref={body} onScroll={onScroll} {...(part(`body`) as Record<string, string>)} className={`${(part(`body`) as { className: string }).className} xui-codeblock-scroll`} data-wrap={wrap ? `true` : undefined} style={maxLines > 0 ? ({ "--xui-code-lines": maxLines } as React.CSSProperties) : undefined} data-max-lines={maxLines > 0 ? maxLines : undefined} tabIndex={0}>
        <pre className="xui-codeblock-pre" id={`${domId}.code`}>
          <code>
            {win.before > 0 ? <span className="xui-codeblock-gap" style={{ height: win.before }} aria-hidden="true" /> : null}
            {shown}
            {win.after > 0 ? <span className="xui-codeblock-gap" style={{ height: win.after }} aria-hidden="true" /> : null}
          </code>
        </pre>
      </div>
    </div>
  )
}
