import type { ReactNode } from "react"
import { createElement } from "react"
import { common, createLowlight } from "lowlight"

// EXP-895 — the diff view's syntax highlighting, PRIVATE to the package (it is
// deliberately absent from `index.ts`): `lowlight` with the same `common`
// grammar set the tiptap code blocks use. Token colours come from the shared
// `--hljs-*` theme variables through the `.diff-code` scope in styles.css.
// Highlighting is per diff LINE, which is cheap and good enough for diffs
// (multi-line constructs may colour slightly off at hunk edges).
//
// Moved here from apps/web `components/diff-view.tsx` so the one file-diff
// renderer owns it; nothing outside `file-diff-card.tsx` imports it.

const lowlight = createLowlight(common)

/** Filename extension → highlight.js grammar (only names present in `common`). */
export const EXT_TO_LANG: Record<string, string> = {
  bash: `bash`,
  sh: `bash`,
  zsh: `bash`,
  c: `c`,
  h: `c`,
  cc: `cpp`,
  cpp: `cpp`,
  cxx: `cpp`,
  hpp: `cpp`,
  cs: `csharp`,
  css: `css`,
  go: `go`,
  gql: `graphql`,
  graphql: `graphql`,
  htm: `xml`,
  html: `xml`,
  svg: `xml`,
  xml: `xml`,
  ini: `ini`,
  toml: `ini`,
  java: `java`,
  cjs: `javascript`,
  js: `javascript`,
  jsx: `javascript`,
  mjs: `javascript`,
  json: `json`,
  kt: `kotlin`,
  kts: `kotlin`,
  less: `less`,
  lua: `lua`,
  m: `objectivec`,
  md: `markdown`,
  markdown: `markdown`,
  pl: `perl`,
  php: `php`,
  py: `python`,
  r: `r`,
  rb: `ruby`,
  rs: `rust`,
  scss: `scss`,
  sql: `sql`,
  swift: `swift`,
  cts: `typescript`,
  mts: `typescript`,
  ts: `typescript`,
  tsx: `typescript`,
  vb: `vbnet`,
  yaml: `yaml`,
  yml: `yaml`,
}

/** The grammar a path highlights with, or null when there is none registered. */
export function languageFor(path: string): string | null {
  const base = path.split(`/`).pop() ?? path
  if (/^makefile$/i.test(base)) return `makefile`
  const dot = base.lastIndexOf(`.`)
  const ext = dot > 0 ? base.slice(dot + 1).toLowerCase() : null
  const lang = ext ? EXT_TO_LANG[ext] : null
  return lang && lowlight.registered(lang) ? lang : null
}

type HastRoot = ReturnType<typeof lowlight.highlight>
type HastChild = HastRoot[`children`][number]

/** hast (lowlight's output) → react nodes, class names and all. */
export function hastToReact(
  nodes: Array<HastChild>,
  keyPrefix: string
): ReactNode {
  return nodes.map((node, i) => {
    if (node.type === `text`) return node.value
    if (node.type === `element`) {
      const className = Array.isArray(node.properties?.className)
        ? node.properties.className.join(` `)
        : undefined
      return createElement(
        `span`,
        { key: `${keyPrefix}-${i}`, className },
        hastToReact(node.children, `${keyPrefix}-${i}`)
      )
    }
    return null
  })
}

/** One diff line's content, highlighted. A grammar that throws falls back to
 *  the plain text — a partial line is not valid source. */
export function highlightLine(
  lang: string,
  text: string,
  key: string
): ReactNode {
  if (!text) return text
  try {
    return hastToReact(lowlight.highlight(lang, text).children, key)
  } catch {
    return text
  }
}
