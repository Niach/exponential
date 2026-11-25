// #NEW-140 — Tailwind scans the whole of `@exp/ui`, so each document carried
// every utility the package uses anywhere. A utility is always a literal class
// name; the ones no script of THIS document names are dropped.

/** `css` cut into its rules: the text up to a `;` or a block's `}`. */
function rulesOf(css: string): string[] {
  const rules: string[] = []
  let depth = 0
  let start = 0
  let quote = ``
  for (let i = 0; i < css.length; i++) {
    const c = css[i]
    if (c === `\\`) i++
    else if (quote) {
      if (c === quote) quote = ``
    } else if (c === `"` || c === `'`) quote = c
    else if (c === `{`) depth++
    else if ((c === `}` && --depth === 0) || (c === `;` && depth === 0)) {
      rules.push(css.slice(start, i + 1))
      start = i + 1
    }
  }
  return css.slice(start).trim() ? [...rules, css.slice(start)] : rules
}

/** A selector list cut at its own commas (the minifier merges equal rules). */
function selectorsOf(list: string): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < list.length; i++) {
    const c = list[i]
    if (c === `\\`) i++
    else if (c === `(` || c === `[`) depth++
    else if (c === `)` || c === `]`) depth--
    else if (c === `,` && depth === 0) {
      out.push(list.slice(start, i))
      start = i + 1
    }
  }
  return [...out, list.slice(start)]
}

/** The class a utility's selector starts with, unescaped. */
function utilityClass(selector: string): string | null {
  const match = /^\s*\.((?:\\[0-9a-f]{1,6}\s?|\\.|[\w-])+)/i.exec(selector)
  if (!match) return null
  return match[1].replace(/\\(?:([0-9a-f]{1,6})\s?|(.))/gi, (_, hex: string | undefined, char: string) => (hex ? String.fromCodePoint(Number.parseInt(hex, 16)) : char))
}

/** False only for a class `script` does not name (one it may spell with an
 *  escape counts as named). */
function named(selector: string, script: string): boolean {
  const name = utilityClass(selector)
  return name === null || /[^\w\-:./[\]()%,#!*&>=+@~^$|]/.test(name) || script.includes(name)
}

function keepUsed(css: string, script: string): string {
  return rulesOf(css)
    .map((rule) => {
      const open = rule.indexOf(`{`)
      if (/^\s*@(?:media|supports|container)\b/.test(rule)) {
        const inner = keepUsed(rule.slice(open + 1, -1), script)
        return inner ? `${rule.slice(0, open + 1)}${inner}}` : ``
      }
      return open < 0 || selectorsOf(rule.slice(0, open)).some((selector) => named(selector, script)) ? rule : ``
    })
    .join(``)
}

/** `css` without the utilities `script` never names (`@layer utilities` only:
 *  a hand-written class may be put together at runtime). */
export function pruneUtilities(css: string, script: string): string {
  return rulesOf(css)
    .map((rule) => {
      const layer = /^\s*@layer utilities\s*\{/.exec(rule)
      return layer ? `${layer[0]}${keepUsed(rule.slice(layer[0].length, -1), script)}}` : rule
    })
    .join(``)
}
