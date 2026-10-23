// FEED-131 — the recipes are the ONLY home of the primitives' class strings:
// for every module here, the React component of the same name
// (exponential-ui-react/src/primitives) and its Solid twin
// (exponential-ui-solid/src/primitives) carry NO Tailwind class literal, so
// the two renderers cannot drift apart by a utility.

import { describe, expect, it } from "bun:test"
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"

const here = import.meta.dir
const packages = join(here, `..`, `..`, `..`, `..`)
const RENDERERS = [
  join(packages, `exponential-ui-react`, `src`, `primitives`),
  join(packages, `exponential-ui-solid`, `src`, `primitives`),
]

const modules = readdirSync(here)
  .filter((f) => f.endsWith(`.ts`) && !f.endsWith(`.test.ts`) && f !== `index.ts`)
  .map((f) => f.slice(0, -3))

// A utility: optional variants (`hover:`, `data-[…]:`, `max-sm:`…), then a
// known utility root, bare or with a value. The roots are spelled in pieces
// so this file never hands Tailwind's scanner a candidate.
const ROOTS = [
  `fl` + `ex`, `gr` + `id`, `inline-fl` + `ex`, `blo` + `ck`, `hid` + `den`, `invis` + `ible`,
  `rel` + `ative`, `abs` + `olute`, `fix` + `ed`, `sr-o` + `nly`, `trun` + `cate`, `pe` + `er`,
  `rou` + `nded`, `te` + `xt`, `b` + `g`, `p[xytrbl]?`, `m[xytrbl]?`, `si` + `ze`, `[wh]`,
  `min-[wh]`, `max-[wh]`, `g` + `ap`, `bor` + `der`, `ri` + `ng`, `sha` + `dow`, `opa` + `city`,
  `z`, `to` + `p`, `le` + `ft`, `ri` + `ght`, `bot` + `tom`, `ins` + `et`, `transl` + `ate`,
  `rot` + `ate`, `fo` + `nt`, `lead` + `ing`, `track` + `ing`, `ite` + `ms`, `just` + `ify`,
  `se` + `lf`, `pla` + `ce`, `over` + `flow`, `asp` + `ect`, `co` + `l`, `ro` + `w`,
  `out` + `line`, `fi` + `ll`, `str` + `oke`, `cur` + `sor`, `sel` + `ect`, `pointer-ev` + `ents`,
  `trans` + `ition`, `dur` + `ation`, `ea` + `se`, `anim` + `ate`, `ori` + `gin`, `back` + `drop`,
  `line-cl` + `amp`, `white` + `space`, `bre` + `ak`, `obj` + `ect`, `mot` + `ion`, `shr` + `ink`,
  `gr` + `ow`, `sc` + `roll`, `tou` + `ch`, `fie` + `ld`, `res` + `ize`, `tab` + `ular`,
]
const UTILITY = new RegExp(
  String.raw`^!?(?:[a-z0-9@*\[\]=&_>.'():/-]+:)*!?-?(?:${ROOTS.join(`|`)})(?:-[^\s]+)?$|^!?(?:[a-z0-9@*\[\]=&_>.'():/-]+:)*\[[^\s]+\]$`
)

function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, ``).replace(/(^|[^:"'`])\/\/.*$/gm, `$1`)
}

/** Every quoted / template literal (template expressions blanked) and every
 *  JSX `class`/`className` string attribute. */
function literals(source: string): string[] {
  const out: string[] = []
  // A JSX attribute other than `class`/`className` is a prop value
  // (`data-slot="select-item"`), never classes.
  const code = stripComments(source).replace(/\b(?!class(?:Name)?=)([\w-]+)="[^"]*"/g, `$1=""`)
  for (const m of code.matchAll(/`((?:[^`\\]|\\.)*)`|"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)'/g)) {
    out.push((m[1] ?? m[2] ?? m[3] ?? ``).replace(/\$\{[^}]*\}/g, ` `))
  }
  return out
}

/** A literal reads as classes: two utilities, or a lone one with a value or
 *  a variant (`size-4`, `max-sm:hidden`), or the lone layout words. */
function classLiteral(text: string): boolean {
  const tokens = text.trim().split(/\s+/).filter(Boolean)
  const utilities = tokens.filter((t) => UTILITY.test(t))
  if (utilities.length >= 2) return true
  if (tokens.length !== 1 || utilities.length !== 1) return false
  const [token] = utilities as [string]
  return /[-:[]/.test(token) || [`hid` + `den`, `sr-o` + `nly`, `rel` + `ative`, `invis` + `ible`, `fl` + `ex`].includes(token)
}

function classAttributes(source: string): string[] {
  const code = stripComments(source)
  return [...code.matchAll(/\bclass(?:Name)?=(?:"([^"]*)"|'([^']*)'|\{`([^`]*)`\})/g)].map((m) => m[1] ?? m[2] ?? m[3] ?? ``)
}

function violations(source: string): string[] {
  return [...literals(source).filter(classLiteral), ...classAttributes(source).filter((c) => c.trim() !== `` && !classLiteral(c))]
}

describe(`the recipes own every class string`, () => {
  it(`spots class literals and leaves prop values alone`, () => {
    const lit = [`fl` + `ex`, `items-center`].join(` `)
    expect(violations(`const a = \`${lit}\``)).toHaveLength(1)
    expect(violations(`<span className="${`sr-o` + `nly`}" />`)).toHaveLength(1)
    expect(violations(`cn(\`${`px-` + `6`}\`, className)`)).toHaveLength(1)
    expect(violations(`variant = \`default\`; size = \`icon-xs\`; side = \`bottom\`; position = \`item-aligned\``)).toEqual([])
    expect(violations(`// ${lit}\nconst a = 1`)).toEqual([])
  })

  for (const name of modules) {
    for (const dir of RENDERERS) {
      const file = [`.tsx`, `.ts`].map((ext) => join(dir, `${name}${ext}`)).find(existsSync)
      if (!file) continue
      it(`${file.slice(packages.length + 1)} has no class literal`, () => {
        expect(violations(readFileSync(file, `utf8`))).toEqual([])
      })
    }
  }
})
