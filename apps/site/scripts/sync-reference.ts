/* Reference pages are GENERATED from the repo's docs/*.md (the single
   source of truth; edit the source, never the copy). Each entry becomes a
   Starlight page with frontmatter; repo-relative links point at GitHub.
   Runs before `astro dev` and `astro build`; the outputs are gitignored. */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join, posix, resolve } from "node:path"

const SITE = resolve(import.meta.dir, `..`)
const REPO = resolve(SITE, `../..`)
const BLOB = `https://github.com/Niach/exponential/blob/master/`

export interface ReferenceSource {
  /** Repo-relative markdown file. */
  source: string
  /** Output under src/content/docs/. */
  page: string
  title: string
  description: string
}

export const REFERENCE: ReferenceSource[] = [
  {
    source: `docs/spec/motion.md`,
    page: `docs/libraries/motion/reference.md`,
    title: `Motion reference`,
    description: `The Motion component in full: the dialect, Lottie support, the engine, playback, state machines, theming and accessibility.`,
  },
]

/** The source minus its H1, with repo-relative links made absolute. */
export function toPage(entry: ReferenceSource, markdown: string): string {
  const body = markdown.replace(/^# .*\n+/, ``)
  const dir = posix.dirname(entry.source)
  const linked = body.replace(/\]\((?!https?:|#|mailto:)([^)\s]+)\)/g, (_, href: string) => `](${BLOB}${posix.normalize(posix.join(dir, href))})`)
  const front = [
    `---`,
    `title: ${JSON.stringify(entry.title)}`,
    `description: ${JSON.stringify(entry.description)}`,
    `source: ${entry.source}`,
    `editUrl: ${BLOB.replace(`/blob/`, `/edit/`)}${entry.source}`,
    `---`,
    ``,
    ``,
  ]
  return front.join(`\n`) + linked
}

if (import.meta.main) {
  for (const entry of REFERENCE) {
    const out = join(SITE, `src/content/docs`, entry.page)
    mkdirSync(dirname(out), { recursive: true })
    writeFileSync(out, toPage(entry, readFileSync(join(REPO, entry.source), `utf8`)))
    console.log(`reference: ${entry.source} → ${entry.page}`)
  }
}
