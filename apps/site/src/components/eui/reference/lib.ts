/* Shared helpers of the genUI reference pages (themes, conformance, numbers,
   eval): repo links, the component page of a catalog name, backtick prose
   as HTML. Everything here runs at build time. */
import componentsDoc from "@exponential_app/eui/docs/components.generated.json"
import { euiComponentPath } from "../../../lib/eui-components.ts"

/** A repo path on GitHub (master). */
export const repo = (path: string) => `https://github.com/Niach/exponential/blob/master/${path}`

const BY_NAME = new Map(componentsDoc.components.map((c) => [c.name, euiComponentPath(c)]))

/** The reference page of a catalog component by its name. */
export const componentPath = (name: string): string => {
  const path = BY_NAME.get(name)
  if (!path) throw new Error(`eui reference: no component page for ${name}`)
  return path
}

const escape = (s: string) => s.replace(/&/g, `&amp;`).replace(/</g, `&lt;`).replace(/>/g, `&gt;`).replace(/"/g, `&quot;`)

/** Prose that marks code with backticks, as HTML (escaped). */
export const ticks = (text: string) =>
  text
    .split(`\``)
    .map((part, i) => (i % 2 ? `<code>${escape(part)}</code>` : escape(part)))
    .join(``)

/** A count the way the pages print it (en-US). */
export const n = (x: number) => x.toLocaleString(`en-US`)
