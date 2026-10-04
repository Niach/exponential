import { SESSION_RESULT_FILES_MAX } from "@exp/db-schema/domain"

// EXP-1154: a run's PR body = a TEXT projection of its Results report
// (`coding_sessions.results`). GitHub cannot render our pictures (every
// attachment read is member-only), so the body carries the report's words
// and file lists and closes with one link to the issue's Results page, where
// the screenshots live. Pure + fixture-locked
// (`packages/domain-contract/fixtures/pr-body.json`); the I/O half is
// `run-pr-body.ts`.
//
// It parses the raw column on its own, like every results reader: an array
// or its JSON string; text sections only (a topic with a non-blank text), the
// FIRST text per topic, first-seen order, `files` cleaned the same way the
// readers clean them.

/** The `pr_open` / `pr_update` body limit. */
export const PR_BODY_MAX = 60_000

export const PR_BODY_RESULTS_LINK_LABEL = `Report and screenshots in Exponential`

interface ReportSection {
  topic: string
  text: string
  files: string[]
}

function cleanFiles(raw: unknown): string[] {
  if (!Array.isArray(raw)) return []
  const out: string[] = []
  const seen = new Set<string>()
  for (const value of raw) {
    if (typeof value !== `string`) continue
    // A path is one line: a newline would break its bullet.
    const path = value.replace(/[\r\n]/g, ``).trim()
    if (!path || seen.has(path)) continue
    seen.add(path)
    out.push(path)
    if (out.length >= SESSION_RESULT_FILES_MAX) break
  }
  return out
}

function parseReportSections(raw: unknown): ReportSection[] {
  let list: unknown = raw
  if (typeof list === `string`) {
    try {
      list = JSON.parse(list)
    } catch {
      return []
    }
  }
  if (!Array.isArray(list)) return []
  const sections: ReportSection[] = []
  const seen = new Set<string>()
  for (const row of list) {
    if (!row || typeof row !== `object` || Array.isArray(row)) continue
    const entry = row as Record<string, unknown>
    if (typeof entry.topic !== `string` || !entry.topic.trim()) continue
    // A picture (attachment id set) is never text, even with a text field.
    if (typeof entry.attachmentId === `string` && entry.attachmentId) continue
    if (typeof entry.text !== `string`) continue
    const text = entry.text.trim()
    if (!text || seen.has(entry.topic)) continue
    seen.add(entry.topic)
    sections.push({ topic: entry.topic, text, files: cleanFiles(entry.files) })
  }
  return sections
}

const isSummary = (topic: string) => topic.trim().toLowerCase() === `summary`

// A code-span bullet: a path like `__init__.py` would otherwise render bold.
// The fence is one backtick longer than the longest run inside the path, and
// a path starting or ending in a backtick is padded so the fence stays apart.
function codeSpan(path: string): string {
  const longest = Math.max(0, ...(path.match(/`+/g) ?? []).map((run) => run.length))
  const fence = `\``.repeat(longest + 1)
  const pad = path.startsWith(`\``) || path.endsWith(`\``) ? ` ` : ``
  return `${fence}${pad}${path}${pad}${fence}`
}

const fileBullets = (files: string[]) => files.map((path) => `- ${codeSpan(path)}`).join(`\n`)

// A heading is one line, and a trailing ` #` run would read as GFM's closing
// sequence and vanish: its last `#` is escaped.
function headingText(topic: string): string {
  const line = topic.replace(/\s+/g, ` `).trim()
  return /(^|\s)#+$/.test(line) ? `${line.slice(0, -1)}\\#` : line
}

function renderSection(section: ReportSection, heading: boolean): string {
  const parts: string[] = []
  if (heading) parts.push(`### ${headingText(section.topic)}`)
  parts.push(section.text)
  if (section.files.length) parts.push(fileBullets(section.files))
  return parts.join(`\n\n`)
}

/**
 * The PR body for a run's report, or null when the report has no text
 * section (the caller then keeps its fallback body). The first `Summary`
 * (case-insensitive) leads without a heading wherever it was filed; every
 * other topic is a `###` section. Over `max` the sections are cut at the last
 * line that fits and end in `…`; the footer link is always kept.
 */
export function prBodyFromResults(
  raw: unknown,
  opts: { resultsUrl: string | null; max?: number }
): string | null {
  const sections = parseReportSections(raw)
  if (!sections.length) return null
  const leadIndex = sections.findIndex((section) => isSummary(section.topic))
  const ordered =
    leadIndex === -1
      ? sections
      : [sections[leadIndex]!, ...sections.filter((_, i) => i !== leadIndex)]
  let main = ordered
    .map((section, i) => renderSection(section, !(i === 0 && leadIndex !== -1)))
    .join(`\n\n`)
  // The blank line before `---` matters: right under a paragraph GFM reads it
  // as a setext heading underline.
  const footer = opts.resultsUrl
    ? `\n\n---\n\n[${PR_BODY_RESULTS_LINK_LABEL}](${opts.resultsUrl})`
    : ``
  const max = opts.max ?? PR_BODY_MAX
  if (main.length + footer.length > max) {
    const budget = Math.max(0, max - footer.length - 2)
    const cut = main.slice(0, budget)
    const lastLine = cut.lastIndexOf(`\n`)
    main = `${(lastLine > 0 ? cut.slice(0, lastLine) : cut).trimEnd()}\n…`
  }
  return `${main}${footer}`
}
