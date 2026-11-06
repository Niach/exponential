import { useInsertionEffect } from "react"
import { BuiltinMarkdown, resolveThemeInput, type HostPlugin } from "@exponential_app/eui-react"
import { BASE_CSS, compiledTheme, partClass, propAttribute } from "@exponential_app/eui/web"
import type { MarkdownOptions } from "@exponential_app/eui/web"
import { cn } from "./cn"

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
const SHEET = `${BASE_CSS}\n${THEME.css}`
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
}: {
  text: string
  /** The host hooks + `markdownOptions` (density, breaks, streaming, image cap). */
  host?: HostPlugin
  className?: string
  ariaLabel?: string
  id?: string
}) {
  useAppMarkdownSheet()
  const density = host?.markdownOptions?.density
  return (
    <div className={cn(`xui-surface`, THEME.scope, `exp-markdown-view min-w-0`, className)} data-xui-mode="dark" data-xui-theme={RESOLVED.id} aria-label={ariaLabel}>
      <div className={ROOT} data-xui-c="Markdown" data-xui-part="Markdown/root" {...(density ? { [DENSITY]: density } : {})}>
        <BuiltinMarkdown text={text} host={host} scope={THEME.scope} theme={RESOLVED} id={id} />
      </div>
    </div>
  )
}
