// EXP-391 — gates for the copy the outside world reads first.
//
// The em dash used as a general-purpose clause connector ("state — remedy",
// "Title — Site") is the loudest "written by an LLM" tell we ship, so it is
// banned from user-facing copy. Guarding every string in the monorepo would
// need a comment-stripping scanner in four languages and would collect an
// allowlist; instead this locks the two hand-maintained DATA surfaces where
// there is no comment/copy ambiguity at all and the blast radius is largest:
// marketing's SEO manifest (meta descriptions, OG and Twitter cards, JSON-LD,
// dist/sitemap.xml and dist/llms.txt all derive from it) and the prerendered
// <title> tags. Sibling of icons.test.ts and third-party-licences.test.ts —
// read repo files, compare, fail loudly.
//
// The <title> parity assertion earns its keep independently of EXP-391:
// scripts/prerender.tsx strips and re-injects og:title/twitter:title from
// PAGES, but never rewrites <title>, so the tab title and the social-card
// title can silently disagree. Nothing else checks that.

import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const read = (path: string) => readFileSync(join(repoRoot, path), `utf8`)

const EM_DASH = `—`

const seo = read(`apps/marketing/src/lib/seo.ts`)

/* Comments are out of scope — this repo writes them with em dashes as a house
   style, including the block comment at the top of seo.ts itself. Continuation
   lines there carry no leading star, so blanking whole block-comment regions
   (keeping the newlines, so reported line numbers stay honest) is the only
   filter that actually holds. */
const codeLines = (source: string) =>
  source
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replaceAll(/[^\n]/g, ` `))
    .split(`\n`)
    .map((line, index) => ({ line, number: index + 1 }))
    .filter(({ line }) => !line.trimStart().startsWith(`//`))

/* Each PAGES entry declares `htmlFile` and, a few keys later, `title`. The
   `sources` array in between holds no `title:` key, so the lazy match cannot
   run past the entry it started in. */
const declaredTitles = [
  ...seo.matchAll(/htmlFile: `([^`]+)`[\s\S]*?\n\s*title: `([^`]+)`/g),
].map((match) => ({ htmlFile: match[1], title: match[2] }))

const titleTag = (html: string) => {
  const match = html.match(/<title>([\s\S]*?)<\/title>/)
  return match ? match[1].replaceAll(`&amp;`, `&`).trim() : undefined
}

describe(`marketing SEO copy`, () => {
  it(`declares a title for every prerendered shell`, () => {
    // A regex that silently matched nothing would make every other assertion
    // here vacuous.
    expect(declaredTitles.length).toBeGreaterThanOrEqual(17)
  })

  it(`has no em dashes outside comments`, () => {
    // PAGES titles and descriptions become the meta description, the OG and
    // Twitter cards, the JSON-LD, and every line of dist/llms.txt.
    const offenders = codeLines(seo)
      .filter(({ line }) => line.includes(EM_DASH))
      .map(({ line, number }) => `seo.ts:${number}: ${line.trim()}`)
    expect(
      offenders,
      `use a period, colon, comma or parentheses instead of an em dash`,
    ).toEqual([])
  })

  it(`keeps every prerendered <title> byte-identical to its PAGES entry`, () => {
    const offenders = declaredTitles
      .map(({ htmlFile, title }) => {
        const shell = titleTag(read(`apps/marketing/${htmlFile}`))
        return shell === title ? undefined : `${htmlFile}: <title>${shell}</title> != PAGES title \`${title}\``
      })
      .filter((offender) => offender !== undefined)
    expect(
      offenders,
      `prerender.tsx rewrites og:title but not <title> — edit both`,
    ).toEqual([])
  })

  it(`has no em dashes in the prerendered <title> tags`, () => {
    const offenders = declaredTitles
      .filter(({ htmlFile }) => titleTag(read(`apps/marketing/${htmlFile}`))?.includes(EM_DASH))
      .map(({ htmlFile }) => htmlFile)
    expect(offenders, `page titles separate with a middle dot: \`Page · Exponential\``).toEqual([])
  })
})
