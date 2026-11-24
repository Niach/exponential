import { useInsertionEffect, useLayoutEffect, useRef } from "react"
import { BuiltinMarkdown, resolveThemeInput, type HostPlugin } from "@exponential_app/eui-react"
import { BASE_CSS, compiledTheme, paintHighlights, partClass, propAttribute } from "@exponential_app/eui/web"
import type { MarkdownOptions } from "@exponential_app/eui/web"
import { cn } from "./cn"
import { useColorMode } from "./color-mode"

// #VAPP-258: the app's read-only Markdown (issue descriptions, comments,
// chats, plans, results) IS genUI's Markdown: the same parser and painter
// the catalog's `Markdown` uses, in the built-in `exponential` theme, outside
// a surface. The theme sheet + the base sheet ride ONE `<style>`; each
// Markdown is a scope root (the theme's variables, dark) holding the `Markdown/root` recipe element.
// The sheet goes into the head ONCE, LAST: its cascade layers must be
// declared after the app's Tailwind layers (as a surface's inline sheet is),
// or preflight's resets (lists, headings, margins) would win over xui-base.
// The host (`markdownOptions` + the text-link, image and attachment hooks)
// is the caller's: the app builds it from its synced rows.

const RESOLVED = resolveThemeInput(`exponential`).theme
const THEME = compiledTheme(RESOLVED)
// #EDE-129: a plain text outside a genUI surface (a transcript tool line)
// paints its search marks with the same fills (`AppHighlightText`).
const FIND_TEXT_CSS = `.exp-find-text::highlight(xui-match),.exp-find-text ::highlight(xui-match){background-color:var(--xui-color-match)}.exp-find-text::highlight(xui-match-current),.exp-find-text ::highlight(xui-match-current){background-color:var(--xui-color-matchCurrent)}`
const SHEET = `${BASE_CSS}\n${THEME.css}\n${FIND_TEXT_CSS}`
const SHEET_ID = `xui-app-markdown-${THEME.scope}`

function useAppMarkdownSheet() {
  useInsertionEffect(() => {
    if (document.getElementById(SHEET_ID)) return
    const style = document.createElement(`style`)
    style.id = SHEET_ID
    style.textContent = SHEET
    document.head.appendChild(style)
  }, [])
}
/** The app's rem scale (styles.css `--exp-markdown-zoom`): the theme's px
 *  sizes at the app's text size, as today's web text. */
const ZOOM = { zoom: `var(--exp-markdown-zoom, 1)` } as const
const DENSITY = propAttribute(false, `density`)
const ROOT = `xui-el ${THEME.scope} ${partClass(`Markdown`, `root`)}`

export type AppMarkdownOptions = MarkdownOptions
/** The host an AppMarkdown takes (genUI's React host plugin). */
export type AppMarkdownHost = HostPlugin

export function AppMarkdown({
  text,
  host,
  className,
  ariaLabel,
  id,
  highlight,
}: {
  text: string
  /** #EDE-95/#EDE-129: genUI's `highlight` ({query, current?}) painted over
   *  the rendered markdown (find in chat). */
  highlight?: unknown
  /** The host hooks + `markdownOptions` (density, breaks, streaming, image cap). */
  host?: HostPlugin
  className?: string
  ariaLabel?: string
  id?: string
}) {
  useAppMarkdownSheet()
  const mode = useColorMode()
  const density = host?.markdownOptions?.density
  const rootRef = useRef<HTMLDivElement>(null)
  usePaintHighlights(rootRef, highlight, true, text)
  return (
    <div ref={rootRef} className={cn(`xui-surface`, THEME.scope, `exp-markdown-view min-w-0`, className)} style={ZOOM} data-xui-mode={mode} data-xui-theme={RESOLVED.id} aria-label={ariaLabel}>
      <div className={ROOT} data-xui-c="Markdown" data-xui-part="Markdown/root" {...(density ? { [DENSITY]: density } : {})}>
        <BuiltinMarkdown text={text} host={host} scope={THEME.scope} theme={RESOLVED} id={id} />
      </div>
    </div>
  )
}

/** #EDE-129: paints `highlight` over `ref`'s rendered text, again whenever
 *  the text or the spec changes (genUI's `paintHighlights`). */
function usePaintHighlights(ref: { current: Element | null }, highlight: unknown, markdown: boolean, text: string) {
  const key = highlight === undefined || highlight === null ? `` : JSON.stringify(highlight)
  useLayoutEffect(() => {
    const el = ref.current
    if (!key || !el) return
    return paintHighlights(el, highlight, markdown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, markdown, text])
}

const INLINE_SURFACE = { display: `inline`, font: `inherit`, color: `inherit`, lineHeight: `inherit` } as const

/** #EDE-129: a plain line of text with genUI's search marks (a transcript
 *  tool line's title); `highlight` absent = the bare text. */
export function AppHighlightText({ text, highlight, className }: { text: string; highlight?: unknown; className?: string }) {
  useAppMarkdownSheet()
  const mode = useColorMode()
  const ref = useRef<HTMLSpanElement>(null)
  usePaintHighlights(ref, highlight, false, text)
  return (
    <span
      ref={ref}
      className={cn(highlight ? `xui-surface ${THEME.scope} exp-find-text` : undefined, className)}
      // The theme's match fills live on its surface; the line keeps its own type.
      style={highlight ? INLINE_SURFACE : undefined}
      data-xui-mode={highlight ? mode : undefined}
      data-xui-theme={highlight ? RESOLVED.id : undefined}
    >
      {text}
    </span>
  )
}
