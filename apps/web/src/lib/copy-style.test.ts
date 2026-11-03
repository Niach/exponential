// EXP-391 — gates for the copy the outside world reads first.
//
// The em dash used as a general-purpose clause connector ("state — remedy",
// "Title — Site") is the loudest "written by an LLM" tell we ship, so it is
// banned from user-facing copy. Guarding every string in the monorepo would
// need a comment-stripping scanner in four languages and would collect an
// allowlist; instead this locks the hand-maintained DATA surfaces of
// exponential.at (apps/site) where there is no comment/copy ambiguity at all
// and the blast radius is largest: the landing SEO helpers (JSON-LD, the full
// <title>, OG and Twitter cards) and every page's `title:` / `description:`
// (the <title>, the meta description, the social cards and the search index).
// Sibling of icons.test.ts and third-party-licences.test.ts — read repo
// files, compare, fail loudly.

import { describe, expect, it } from "vitest"
import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`, `..`)
const read = (path: string) => readFileSync(join(repoRoot, path), `utf8`)

const EM_DASH = `—`
const SEO = `apps/site/src/components/landing/seo.ts`
const PAGES = `apps/site/src/pages`

/* Comments are out of scope — this repo writes them with em dashes as a house
   style. Blank whole block-comment regions (keeping the newlines, so reported
   line numbers stay honest), then drop line comments. */
const codeLines = (source: string) =>
  source
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replaceAll(/[^\n]/g, ` `))
    .split(`\n`)
    .map((line, index) => ({ line, number: index + 1 }))
    .filter(({ line }) => !line.trimStart().startsWith(`//`))

const astroPages = (dir: string): string[] =>
  readdirSync(join(repoRoot, dir), { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? astroPages(`${dir}/${entry.name}`)
      : entry.name.endsWith(`.astro`)
        ? [`${dir}/${entry.name}`]
        : []
  )

/* Every `title: \`…\`` / `description: \`…\`` a page declares (the StarlightPage
   frontmatter and its seoHead call). */
const declared = astroPages(PAGES).flatMap((path) =>
  [...read(path).matchAll(/\b(title|description): `([^`]+)`/g)].map((match) => ({
    path,
    key: match[1],
    value: match[2],
  }))
)

describe(`exponential.at SEO copy`, () => {
  it(`finds the pages' titles and descriptions`, () => {
    // A regex that silently matched nothing would make the next assertion
    // vacuous.
    expect(declared.filter(({ key }) => key === `title`).length).toBeGreaterThanOrEqual(10)
  })

  it(`has no em dashes in a page title or description`, () => {
    const offenders = declared
      .filter(({ value }) => value.includes(EM_DASH))
      .map(({ path, key, value }) => `${path}: ${key} \`${value}\``)
    expect(
      offenders,
      `use a period, colon, comma or parentheses instead of an em dash; page titles separate with a middle dot: \`Page · Exponential\``,
    ).toEqual([])
  })

  it(`has no em dashes in the SEO helpers outside comments`, () => {
    const offenders = codeLines(read(SEO))
      .filter(({ line }) => line.includes(EM_DASH))
      .map(({ line, number }) => `seo.ts:${number}: ${line.trim()}`)
    expect(
      offenders,
      `use a period, colon, comma or parentheses instead of an em dash`,
    ).toEqual([])
  })
})
