/**
 * #EXP-1674: a bare in-app URL in agent prose reads as the chip of what it
 * names (`fixtures/prose-links.json`, whose `description` is the rule; desktop
 * `domain::prose_links`, iOS `ProseLinks.swift`, Android `ProseLinks.kt`).
 * Display text only: nothing here ever serializes.
 */

import { classifyAppLink } from "./app-link"

export type ProseChipKind = `issue` | `board` | `session` | `agent`

/** The display title of a SYNCED row, null otherwise. */
export type ProseChipResolver = (kind: ProseChipKind, id: string) => string | null | undefined

const MARKER = `#exp-chip-`
const STOPS = ` \t<>()[]\`"'`
const TRAILING = `.,;:!?*_~`

function isWord(ch: string | undefined): boolean {
  return ch !== undefined && /^[A-Za-z0-9_]$/.test(ch)
}

function isFenceLine(line: string): boolean {
  let at = 0
  while (at < 3 && line[at] === ` `) at += 1
  const rest = line.slice(at)
  return rest.startsWith("```") || rest.startsWith(`~~~`)
}

function escapeTitle(title: string): string {
  return title.replace(/[\\[\]*_`~]/g, (ch) => `\\${ch}`)
}

function chipOf(url: string, origin: string, team: string, resolve: ProseChipResolver): string | null {
  const link = classifyAppLink(url, origin, team)
  if (!(`teamSlug` in link) || link.teamSlug.toLowerCase() !== team.toLowerCase()) return null
  let kind: ProseChipKind
  let id: string
  switch (link.kind) {
    case `issue`:
    case `issueRef`:
      kind = `issue`
      id = link.identifier
      break
    case `board`:
      kind = `board`
      id = link.boardSlug
      break
    case `session`:
      kind = `session`
      id = link.sessionId
      break
    case `agent`:
      kind = `agent`
      id = link.agentId
      break
    default:
      return null
  }
  const title = resolve(kind, id)
  if (title === null || title === undefined) return null
  if (kind === `issue`) return `#${id}`
  const hash = url.indexOf(`#`)
  return `[${escapeTitle(title)}](${hash === -1 ? url : url.slice(0, hash)}${MARKER}${kind})`
}

function chipLine(line: string, origin: string, team: string, resolve: ProseChipResolver): string {
  let out = ``
  let at = 0
  while (at < line.length) {
    const ch = line[at]!
    if (ch === "`") {
      let run = at
      while (line[run] === "`") run += 1
      const width = run - at
      let close = run
      let end = -1
      while (close < line.length) {
        if (line[close] !== "`") {
          close += 1
          continue
        }
        let closeRun = close
        while (line[closeRun] === "`") closeRun += 1
        if (closeRun - close === width) {
          end = closeRun
          break
        }
        close = closeRun
      }
      const stop = end === -1 ? run : end
      out += line.slice(at, stop)
      at = stop
      continue
    }
    const lower = line.slice(at, at + 8).toLowerCase()
    if (lower.startsWith(`http://`) || lower.startsWith(`https://`)) {
      const prev = at > 0 ? line[at - 1] : undefined
      const destination = prev === `(` && at > 1 && line[at - 2] === `]`
      if (!isWord(prev) && prev !== `[` && prev !== `<` && !destination) {
        let end = at
        while (end < line.length && !STOPS.includes(line[end]!)) end += 1
        while (end > at && TRAILING.includes(line[end - 1]!)) end -= 1
        const url = line.slice(at, end)
        const chip = chipOf(url, origin, team, resolve)
        out += chip ?? url
        at = end
        continue
      }
    }
    out += ch
    at += 1
  }
  return out
}

/** `text` with its bare in-app URLs rewritten to their chips' sources. */
export function chipBareAppLinks(
  text: string,
  origin: string,
  team: string | null | undefined,
  resolve: ProseChipResolver
): string {
  if (!team) return text
  const lowered = text.toLowerCase()
  if (!lowered.includes(`http://`) && !lowered.includes(`https://`)) return text
  let fence: string | null = null
  return text
    .split(`\n`)
    .map((line) => {
      if (isFenceLine(line)) {
        const mark = line.trimStart().slice(0, 3)
        if (fence === null) fence = mark
        else if (fence === mark) fence = null
        return line
      }
      return fence === null ? chipLine(line, origin, team, resolve) : line
    })
    .join(`\n`)
}

/** The chip kind a rewritten link's href carries (null: an ordinary link). */
export function proseChipKind(href: string): ProseChipKind | null {
  const at = href.lastIndexOf(MARKER)
  if (at === -1) return null
  const kind = href.slice(at + MARKER.length)
  return kind === `board` || kind === `session` || kind === `agent` ? kind : null
}
