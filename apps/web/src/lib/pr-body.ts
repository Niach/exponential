import { SESSION_RESULT_FILES_MAX } from "@exp/db-schema/domain"

import type { GuideEntry } from "@exp/db-schema/domain"

// EXP-1154/1291: every PR body of a thread = a TEXT projection of the
// thread's ONE Guide (root `sessions.guide`). GitHub cannot render our
// pictures (attachment reads are member-only), so the body carries the
// Guide's words and file lists and closes with one link to the Guide, where
// the screenshots live. A topic tagged with a `prUrl` belongs to that PR; a
// PR's body keeps its own topics plus the untagged ones. Pure +
// fixture-locked (`packages/domain-contract/fixtures/pr-body.json`); the I/O
// half is `lib/pull-requests.ts` (`syncPullRequestBodies`).
//
// It parses the raw value on its own, like every Guide reader: an array or
// its JSON string; text sections only (a topic with a non-blank text), the
// FIRST text per topic, first-seen order, `files` cleaned the same way the
// readers clean them. `sessionIds` (EXP-1284, still fixture-locked) narrows
// to one session's entries; the thread-wide body never passes it.

/** The `pr_open` / `pr_update` body limit. */
export const PR_BODY_MAX = 60_000

export const PR_BODY_RESULTS_LINK_LABEL = `Guide and screenshots in Exponential`

/** EXP-1315: the Guide topic `pr_open`'s `notRun` is filed under (the UI
 *  screens the agent could not run); the PR body renders it LAST. */
export const NOT_RUN_TOPIC = `Not run`

export const isNotRunTopic = (topic: string) =>
  topic.replace(/\s+/g, ` `).trim().toLowerCase() === NOT_RUN_TOPIC.toLowerCase()

interface ReportSection {
  topic: string
  text: string
  files: string[]
  prUrl: string | null
  sessionId: string | null
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

function parseReportSections(
  raw: unknown,
  keep?: (section: ReportSection) => boolean
): ReportSection[] {
  let list: unknown = raw
  if (typeof list === `string`) {
    try {
      list = JSON.parse(list)
    } catch {
      return []
    }
  }
  if (!Array.isArray(list)) return []
  const candidates: ReportSection[] = []
  for (const row of list) {
    if (!row || typeof row !== `object` || Array.isArray(row)) continue
    const entry = row as Record<string, unknown>
    if (typeof entry.topic !== `string` || !entry.topic.trim()) continue
    // A picture (attachment id set) is never text, even with a text field.
    if (typeof entry.attachmentId === `string` && entry.attachmentId) continue
    if (typeof entry.text !== `string`) continue
    const text = entry.text.trim()
    if (!text) continue
    const prUrl = typeof entry.prUrl === `string` ? entry.prUrl.trim() || null : null
    const sessionId =
      typeof entry.sessionId === `string` ? entry.sessionId.trim() || null : null
    const section = { topic: entry.topic, text, files: cleanFiles(entry.files), prUrl, sessionId }
    if (!keep || keep(section)) candidates.push(section)
  }
  // Under a session filter, a topic a kept stamped entry has leaves its
  // legacy (unstamped) twin out.
  const stampedTopics = new Set(
    keep
      ? candidates.filter((section) => section.sessionId).map((section) => section.topic)
      : []
  )
  const sections: ReportSection[] = []
  const seen = new Set<string>()
  for (const section of candidates) {
    if (seen.has(section.topic)) continue
    if (!section.sessionId && stampedTopics.has(section.topic)) continue
    seen.add(section.topic)
    sections.push(section)
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
 *
 * `prUrl` (EXP-1251) = the PR the body is for: a topic tagged with another PR
 * is left out; null = untagged topics only (a PR not opened yet); absent =
 * every topic.
 *
 * `sessionIds` (EXP-1284) = the session the body is for and its resume
 * chain: an entry stamped with ANOTHER session is left out unless it is
 * tagged with this `prUrl`; unstamped entries stay but yield their topic to a
 * kept stamped one. Absent = no session filter.
 */
export function renderPrBody(
  raw: unknown,
  opts: {
    resultsUrl: string | null
    max?: number
    prUrl?: string | null
    sessionIds?: readonly string[]
  }
): string | null {
  const want = opts.prUrl === undefined ? undefined : opts.prUrl?.trim() || null
  const sessionIds = opts.sessionIds
  const sections = parseReportSections(
    raw,
    sessionIds
      ? (section) =>
          !section.sessionId ||
          sessionIds.includes(section.sessionId) ||
          (!!want && section.prUrl === want)
      : undefined
  ).filter(
    (section) => want === undefined || section.prUrl === null || section.prUrl === want
  )
  if (!sections.length) return null
  const leadIndex = sections.findIndex((section) => isSummary(section.topic))
  // EXP-1315: the `Not run` topic (pr_open's notRun) always closes the body.
  const ordered = [
    ...(leadIndex === -1 ? [] : [sections[leadIndex]!]),
    ...sections.filter((section, i) => i !== leadIndex && !isNotRunTopic(section.topic)),
    ...sections.filter((section, i) => i !== leadIndex && isNotRunTopic(section.topic)),
  ]
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

/**
 * EXP-1291: the body of one PR of a thread, off the thread's Guide. `prUrl`
 * = the PR it is for (null = a PR not opened yet: untagged topics only;
 * absent = every topic); `guideUrl` = the footer link. Null without any text
 * section (the caller keeps its fallback).
 */
export function prBodyFromGuide(
  guide: GuideEntry[] | null,
  opts: { prUrl?: string | null; guideUrl?: string | null } = {}
): string | null {
  return renderPrBody(guide, {
    resultsUrl: opts.guideUrl ?? null,
    ...(opts.prUrl !== undefined ? { prUrl: opts.prUrl } : {}),
  })
}

// S-3 (owner 2026-10-28): stories replace the Guide. A PR body = a short
// SERVER summary (the PR title, ≤3 bullets: the linked issues first, then
// the commit subjects) + the session's story pictures grouped by the
// platform word their label starts with + the `Not run` line + the link to
// the thread. No LLM; pure + fixture-locked (pr-body.json `storyCases`).
// GitHub cannot load our member-only pictures, so each is a link.

export const PR_BODY_STORIES_LINK_LABEL = `Stories in Exponential`
export const PR_BODY_BULLETS_MAX = 3
const STORY_PLATFORMS = [`web`, `ios`, `android`, `desktop`] as const

export interface StoryBodyInput {
  title: string
  issues: readonly { identifier: string; title: string }[]
  /** Full commit messages, oldest first. */
  commits: readonly string[]
  pictures: readonly { label: string; url: string }[]
  notRun: string | null
  storiesUrl: string | null
}

const oneLine = (text: string) => text.replace(/\s+/g, ` `).trim()

/** The picture's platform: its label's first word (`web-mobile` = web). */
export function storyPlatform(label: string): string {
  const word = (oneLine(label).split(` `)[0] ?? ``).toLowerCase()
  const platform = STORY_PLATFORMS.find((name) => word === name || word.startsWith(`${name}-`))
  return platform ?? `other`
}

export function prBodyFromStories(input: StoryBodyInput, max = PR_BODY_MAX): string {
  const title = oneLine(input.title)
  const bullets: string[] = []
  const seen = new Set<string>([title.toLowerCase()])
  const add = (text: string) => {
    const line = oneLine(text)
    if (!line || seen.has(line.toLowerCase()) || bullets.length >= PR_BODY_BULLETS_MAX) return
    seen.add(line.toLowerCase())
    bullets.push(line)
  }
  for (const issue of input.issues) add(`#${issue.identifier} ${issue.title}`)
  for (const commit of input.commits) {
    const subject = commit.split(`\n`)[0] ?? ``
    if (/^Merge (branch|pull request|remote-tracking)/i.test(subject)) continue
    add(subject)
  }
  const parts: string[] = [title]
  if (bullets.length) parts.push(bullets.map((line) => `- ${line}`).join(`\n`))
  const groups = new Map<string, string[]>()
  for (const picture of input.pictures) {
    const platform = storyPlatform(picture.label)
    const label = oneLine(picture.label).replace(/[[\]]/g, ``) || platform
    groups.set(platform, [...(groups.get(platform) ?? []), `[${label}](${picture.url})`])
  }
  const lines = [...STORY_PLATFORMS, `other`]
    .filter((platform) => groups.has(platform))
    .map((platform) => `**${platform}** · ${groups.get(platform)!.join(` · `)}`)
  if (lines.length) parts.push(lines.join(`\n`))
  const notRun = input.notRun ? oneLine(input.notRun) : ``
  if (notRun) parts.push(`**${NOT_RUN_TOPIC}:** ${notRun}`)
  let main = parts.join(`\n\n`)
  const footer = input.storiesUrl ? `\n\n---\n\n[${PR_BODY_STORIES_LINK_LABEL}](${input.storiesUrl})` : ``
  if (main.length + footer.length > max) {
    const budget = Math.max(0, max - footer.length - 2)
    const cut = main.slice(0, budget)
    const lastLine = cut.lastIndexOf(`\n`)
    main = `${(lastLine > 0 ? cut.slice(0, lastLine) : cut).trimEnd()}\n…`
  }
  return `${main}${footer}`
}
