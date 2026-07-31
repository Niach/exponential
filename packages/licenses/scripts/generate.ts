#!/usr/bin/env bun
// EXP-375 — the MERGE step: five committed inventories + the curated supplement
// -> one committed NOTICES.txt per distributed client.
//
// Run: `bun run --filter @exp/licenses generate`.
//
// This script is PURE and OFFLINE by design. It reads only committed files and
// writes only committed files, so it runs in the pure-bun `web` CI job, where
// `apps/web/src/lib/licenses.test.ts` re-runs it and byte-compares the outputs.
// The toolchain-dependent half — cargo, node_modules, a gradle resolve, network
// access to pinned Swift revisions — lives in `scripts/collect-*.ts` and runs
// in the CI jobs that already have those toolchains.
//
// Do not introduce a clock, a hostname, a working directory or a locale-aware
// comparison anywhere below. Two runners must agree byte-for-byte.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import {
  COMMERCIAL,
  FONTS,
  ICONS,
  MPL_STATEMENT,
  mplSourceUrl,
  NOT_INCLUDED,
  TRADEMARKS,
  TRADEMARK_STATEMENT,
  VENDORED,
  type Client,
  type CuratedEntry,
} from "../curated/supplement"
import { LICENCE_OVERRIDES, findOverride } from "../curated/overrides"
import { byCodepoint, type Component, type Inventory } from "../src/schema"
import { ELECTION_ORDER } from "../src/spdx"
import { normaliseBody, spdxTemplate } from "../src/text"
import { renderComponents, renderFile, rule, wrap, type Section } from "../src/render"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), "..")
const repoRoot = join(pkgRoot, "..", "..")

/** Where each client's notice ships, and how. */
export const OUTPUTS: Record<
  Client,
  { title: string; path: string; inventories: string[]; ships: string }
> = {
  desktop: {
    title: `Desktop application`,
    path: `apps/desktop/assets/licenses/NOTICES.txt`,
    inventories: [`rust.json`],
    ships: `embedded in the binary`,
  },
  web: {
    title: `Web application`,
    path: `apps/web/public/NOTICES.txt`,
    inventories: [`npm-web.json`],
    ships: `served at /NOTICES.txt`,
  },
  marketing: {
    title: `Marketing site`,
    path: `apps/marketing/public/NOTICES.txt`,
    inventories: [`npm-marketing.json`],
    ships: `served at /NOTICES.txt`,
  },
  ios: {
    title: `iOS application`,
    path: `apps/ios/Exponential/Resources/NOTICES.txt`,
    inventories: [`ios.json`],
    ships: `bundled as an app resource`,
  },
  android: {
    title: `Android application`,
    path: `apps/android/app/src/main/assets/NOTICES.txt`,
    inventories: [`android.json`],
    ships: `packaged in the APK assets`,
  },
}

const readRepoFile = (path: string): string => {
  const file = join(repoRoot, path)
  if (!existsSync(file)) {
    throw new Error(
      `curated supplement points at ${path}, which does not exist — ` +
        `fix packages/licenses/curated/supplement.ts`
    )
  }
  return normaliseBody(readFileSync(file, `utf8`))
}

// ---------------------------------------------------------------------------
// Curated sections
// ---------------------------------------------------------------------------

const forClient = (entries: CuratedEntry[], client: Client): CuratedEntry[] =>
  entries.filter((e) => e.clients.includes(client))

function renderCurated(entries: CuratedEntry[]): string[] {
  const lines: string[] = []
  entries.forEach((entry, i) => {
    if (i > 0) lines.push(``, ``)
    lines.push(rule(`-`), entry.title, rule(`-`), ``)
    entry.body.forEach((para, j) => {
      if (j > 0) lines.push(``)
      lines.push(...wrap(para))
    })
    for (const rep of entry.reproduce ?? []) {
      lines.push(``, rule(`.`), rep.label, rule(`.`), ``)
      lines.push(...readRepoFile(rep.path).replace(/\n$/, ``).split(`\n`))
    }
  })
  return lines
}

/**
 * Section 2 — the election policy, written out so a reader can check our work
 * rather than take it on faith. `self_cell` is the load-bearing case: it offers
 * "Apache-2.0 OR GPL-2.0-only", and a generator that printed both would assert
 * GPL terms over a binary that has none.
 */
function electionSection(components: Component[]): Section | undefined {
  const elected = components
    .filter((c) => c.election)
    .sort((a, b) => byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version))
  if (elected.length === 0) return undefined

  const lines: string[] = []
  lines.push(
    ...wrap(
      `An SPDX expression joined by OR is the licensor offering a CHOICE, not a set of obligations that all apply. This notice elects exactly ONE branch of every such choice and records it. An expression joined by AND is the opposite — every conjunct binds, and all of them are reproduced.`
    )
  )
  lines.push(``)
  lines.push(
    ...wrap(
      `Elections are made by the following preference order, most preferred first:`
    )
  )
  lines.push(``)
  for (const id of ELECTION_ORDER) lines.push(`    ${id}`)
  lines.push(``)
  lines.push(
    ...wrap(
      `No copyleft licence appears in that list. Where an upstream offered one as a branch, the permissive branch was taken; the elections below are the complete record.`
    )
  )
  lines.push(``, ``, rule(`-`), `Elections made in this build`, rule(`-`), ``)
  for (const component of elected) {
    lines.push(`  ${component.name} ${component.version}`)
    lines.push(...wrap(component.election!, `    `))
  }
  return { title: `Licence election`, lines }
}

/** Section — MPL-2.0 section 3.2, derived from the inventory, never hand-listed. */
function mplSection(components: Component[]): Section | undefined {
  const mpl = components
    .filter((c) => c.licenses.includes(`MPL-2.0`) && !c.buildOnly)
    .sort((a, b) => byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version))
  if (mpl.length === 0) return undefined

  const lines: string[] = []
  for (const para of MPL_STATEMENT) lines.push(...wrap(para), ``)
  for (const component of mpl) {
    const url = mplSourceUrl(component.name)
    if (!url) {
      throw new Error(
        `${component.name} is MPL-2.0 and linked, but has no source URL — ` +
          `add one to MPL_SOURCE_URLS in packages/licenses/curated/supplement.ts`
      )
    }
    lines.push(`  ${component.name} ${component.version}`)
    lines.push(...wrap(`Source Code Form: ${url}`, `    `))
    lines.push(...wrap(`Distributed unmodified.`, `    `))
  }
  return {
    title: `Mozilla Public License 2.0 — source availability`,
    lines,
  }
}

function trademarkSection(client: Client): Section | undefined {
  const marks = TRADEMARKS.filter((t) => t.clients.includes(client)).sort(
    (a, b) => byCodepoint(a.mark, b.mark)
  )
  if (marks.length === 0) return undefined
  const lines: string[] = []
  for (const para of TRADEMARK_STATEMENT) lines.push(...wrap(para), ``)
  for (const mark of marks) {
    lines.push(`  ${mark.mark}`)
    lines.push(...wrap(`Owner: ${mark.owner}`, `    `))
    lines.push(...wrap(`Used to ${mark.use}.`, `    `))
  }
  return { title: `Trademarks`, lines }
}

/**
 * Section — non-OSS components. Two sources feed it: curated entries (Remotion)
 * and any inventory component a collector emitted with an EMPTY `licenses`
 * array, which is how the iOS and Android collectors mark something that ships
 * under a vendor's own terms rather than an open-source licence. Neither may
 * ever appear in the open-source aggregate.
 */
function commercialSection(
  client: Client,
  components: Component[]
): { section?: Section; excluded: Component[] } {
  const excluded = components
    .filter((c) => c.licenses.length === 0)
    .sort((a, b) => byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version))
  const curated = forClient(COMMERCIAL, client)
  if (excluded.length === 0 && curated.length === 0) return { excluded }

  const lines: string[] = []
  lines.push(
    ...wrap(
      `The components below are NOT open source. Each is distributed under its own vendor's terms, which are not reproduced here and are not granted by anything else in this file. They are listed separately so that nothing in the open-source sections above is read as covering them.`
    )
  )
  if (excluded.length > 0) {
    lines.push(``, ``, rule(`-`), `Components under vendor terms`, rule(`-`), ``)
    for (const component of excluded) {
      lines.push(`  ${component.name} ${component.version}`)
      lines.push(...wrap(component.declared, `    `))
      if (component.homepage) lines.push(...wrap(`Terms: ${component.homepage}`, `    `))
    }
  }
  if (curated.length > 0) {
    if (excluded.length > 0) lines.push(``, ``)
    lines.push(...renderCurated(curated))
  }
  return { section: { title: `Commercially licensed components`, lines }, excluded }
}

// ---------------------------------------------------------------------------
// Assemble
// ---------------------------------------------------------------------------

function loadInventories(client: Client, files: string[]): Component[] {
  const components: Component[] = []
  for (const file of files) {
    const path = join(pkgRoot, `inventory`, file)
    if (!existsSync(path)) {
      throw new Error(
        `missing inventory/${file} — run the matching \`collect:*\` script ` +
          `(see packages/licenses/README.md)`
      )
    }
    const inventory = JSON.parse(readFileSync(path, `utf8`)) as Inventory
    components.push(...inventory.components)
  }
  return components
    .map((component) => applyOverride(client, component))
    .sort((a, b) => byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version))
}

/** Overrides that fired, so an override matching nothing can be a hard error. */
const overridesUsed = new Set<string>()

function applyOverride(client: Client, component: Component): Component {
  const override = findOverride(client, component.name)
  if (!override) return component
  overridesUsed.add(`${override.scope}:${override.name}`)

  const texts = override.useTemplate
    ? override.licenses.map((spdx) => {
        const body = spdxTemplate(join(pkgRoot, `texts`), spdx)
        if (!body) {
          throw new Error(
            `override for ${component.name} wants a ${spdx} template — ` +
              `add it via \`bun run --filter @exp/licenses fetch:texts\``
          )
        }
        return { spdx, source: `spdx-template`, body }
      })
    : component.texts

  return {
    ...component,
    declared: override.declared,
    licenses: override.licenses,
    ...(override.copyright ? { copyright: override.copyright } : {}),
    texts: override.licenses.length === 0 ? [] : texts,
  }
}

export function buildNotice(client: Client): string {
  const config = OUTPUTS[client]
  const all = loadInventories(client, config.inventories)

  const commercial = commercialSection(client, all)
  const vendorTerms = new Set(
    commercial.excluded.map((c) => `${c.name}@${c.version}`)
  )
  // The open-source aggregate must never contain a vendor-terms component.
  const oss = all.filter((c) => !vendorTerms.has(`${c.name}@${c.version}`))

  const sections: Section[] = []
  if (oss.length > 0) {
    sections.push({ title: `Open-source components`, lines: renderComponents(oss) })
  }
  const election = electionSection(oss)
  if (election) sections.push(election)

  const fonts = forClient(FONTS, client)
  if (fonts.length > 0) {
    sections.push({ title: `Bundled fonts`, lines: renderCurated(fonts) })
  }
  const icons = forClient(ICONS, client)
  if (icons.length > 0) {
    sections.push({ title: `Bundled icons`, lines: renderCurated(icons) })
  }
  const vendored = forClient(VENDORED, client)
  if (vendored.length > 0) {
    sections.push({ title: `Vendored source`, lines: renderCurated(vendored) })
  }
  const mpl = mplSection(oss)
  if (mpl) sections.push(mpl)

  const trademarks = trademarkSection(client)
  if (trademarks) sections.push(trademarks)

  if (commercial.section) sections.push(commercial.section)

  const notIncluded = forClient(NOT_INCLUDED, client)
  if (notIncluded.length > 0) {
    sections.push({
      title: `Components deliberately not included`,
      lines: renderCurated(notIncluded),
    })
  }

  return renderFile({ clientTitle: config.title, sections })
}

if (import.meta.main) {
  for (const client of Object.keys(OUTPUTS).sort(byCodepoint) as Client[]) {
    const config = OUTPUTS[client]
    const contents = buildNotice(client)
    const out = join(repoRoot, config.path)
    mkdirSync(dirname(out), { recursive: true })
    writeFileSync(out, contents)
    const kb = Math.round(contents.length / 1024)
    console.log(`${config.path}  (${kb} KB, ${config.ships})`)
  }

  // An override that matched nothing is a determination about a dependency we
  // no longer have — or a typo. Either way it must not sit there unnoticed.
  const unused = LICENCE_OVERRIDES.filter(
    (o) => !overridesUsed.has(`${o.scope}:${o.name}`)
  )
  if (unused.length > 0) {
    console.error(
      `\nstale entries in packages/licenses/curated/overrides.ts:\n` +
        unused.map((o) => `  ${o.scope}: ${o.name}`).join(`\n`)
    )
    process.exit(1)
  }
}
