/* Reference pages are GENERATED from repo markdown (the single source of
   truth; edit the source, never the copy). Every list in scripts/reference/
   (one file per area) names its sources; each becomes
   src/content/docs/<slug>.gen.md (gitignored) with frontmatter, its H1
   dropped and repo-relative links pointing at GitHub.
   Runs before `astro dev` and `astro build`. */
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join, posix, resolve } from "node:path"

const SITE = resolve(import.meta.dir, `..`)
const REPO = resolve(SITE, `../..`)
const DOCS = join(SITE, `src/content/docs`)
const BLOB = `https://github.com/Niach/exponential/blob/master/`

export interface ReferenceSource {
  /** Repo-relative markdown file. */
  source: string
  /** The page's URL path without slashes, e.g. `docs/libraries/motion/reference`. */
  slug: string
  title: string
  description: string
  /** Optional transform of the markdown body (e.g. cut internal sections). */
  transform?: (markdown: string) => string
}

/** The source minus its H1, with repo-relative links made absolute. */
export function toPage(entry: ReferenceSource, markdown: string): string {
  let body = markdown.replace(/^# .*\n+/, ``)
  if (entry.transform) body = entry.transform(body)
  const dir = posix.dirname(entry.source)
  const linked = body.replace(/\]\((?!https?:|#|mailto:|\/)([^)\s]+)\)/g, (_, href: string) => `](${BLOB}${posix.normalize(posix.join(dir, href))})`)
  const front = [
    `---`,
    `title: ${JSON.stringify(entry.title)}`,
    `description: ${JSON.stringify(entry.description)}`,
    `slug: ${entry.slug}`,
    `source: ${entry.source}`,
    `editUrl: ${BLOB.replace(`/blob/`, `/edit/`)}${entry.source}`,
    `---`,
    ``,
    ``,
  ]
  return front.join(`\n`) + linked
}

function generated(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const path = join(dir, e.name)
    if (e.isDirectory()) return generated(path)
    return e.name.endsWith(`.gen.md`) ? [path] : []
  })
}

if (import.meta.main) {
  for (const file of generated(DOCS)) rmSync(file)
  const lists = readdirSync(join(SITE, `scripts/reference`)).filter((f) => f.endsWith(`.ts`))
  for (const list of lists) {
    const { REFERENCE } = (await import(join(SITE, `scripts/reference`, list))) as { REFERENCE: ReferenceSource[] }
    for (const entry of REFERENCE) {
      const out = join(DOCS, `${entry.slug}.gen.md`)
      mkdirSync(dirname(out), { recursive: true })
      writeFileSync(out, toPage(entry, readFileSync(join(REPO, entry.source), `utf8`)))
      console.log(`reference: ${entry.source} → /${entry.slug}/`)
    }
  }
}
