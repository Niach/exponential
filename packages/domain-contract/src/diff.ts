// EXP-895 — the ONE diff model every client renders, and the ONE parser that
// builds it. Hand-mirrored ×4 (desktop `coding::scm` / `ui::diff`, web
// `lib/diff.ts` consumers, iOS `ExpCore/Sources/Domain/Diff.swift`, Android
// `domain/Diff.kt`) and byte-locked by `fixtures/diff/cases.json` +
// `fixtures/diff/summary.json`, which every client's test replays.
//
// Three producers feed the same model:
//   1. `git diff` output from the desktop worktree (`diff --git` sections),
//   2. the steer relay's per-call tool diffs (bare `--- a/x` / `+++ b/x`
//      sections with no `diff --git` line, optionally cut with a trailing
//      `\ N more lines truncated` marker),
//   3. GitHub's PullFile `patch` (hunks only; the path and status arrive
//      beside it, never inside it) — `parsePatch` / `fromPullFile`.
//
// The parse rules below ARE the contract; a change here is a change on all
// four clients. The projection `renderDiff` is what the fixture freezes, so
// every platform can compare one array of strings instead of a whole object
// graph.

import contractJson from "../contract.json" with { type: "json" }

/** The empty diff's summary — the contract's words. */
const NO_CHANGES: string = (
  contractJson as unknown as { diffUi: { noChanges: string } }
).diffUi.noChanges

// ── Model ───────────────────────────────────────────────────────────────────

export interface DiffLine {
  kind: `add` | `del` | `context` | `meta`
  /** 1-based line number on the old side; absent on `add`/`meta`. */
  oldNo?: number
  /** 1-based line number on the new side; absent on `del`/`meta`. */
  newNo?: number
  /** The line's content, its one-character sign stripped. */
  text: string
}

export interface DiffHunk {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  /** The verbatim `@@ … @@` line, section heading and all. */
  header: string
  lines: DiffLine[]
}

export interface DiffFile {
  path: string
  /** Only on `renamed`/`copied`: where the file came from. */
  previousPath?: string
  status: `added` | `removed` | `modified` | `renamed` | `copied`
  additions: number
  deletions: number
  binary: boolean
  hunks: DiffHunk[]
}

export type DiffStatus = DiffFile[`status`]

export interface Diff {
  files: DiffFile[]
  /** Lines the PUBLISHER dropped, read back off its `\ N more lines
   *  truncated` marker (EXP-786). Absent when the diff is whole. */
  truncatedLines?: number
}

/** Every line number and count saturates here (i32::MAX) — the natives carry
 *  32-bit counters and a hostile `@@` header must never wrap one. */
export const DIFF_LINE_MAX = 2147483647

/** The one marker the steer relay appends to a cut patch (EXP-786); web
 *  `splitTruncatedDiff` and desktop `truncated_marker_count` spell it the
 *  same way. Anchored to the END of the text: only a TRAILING marker counts. */
const TRUNCATION_MARKER = /(?:^|\n)\\ (\d+) more lines? truncated\s*$/

const HUNK_HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/

// Path precedence: a higher-ranked source overwrites a lower-ranked one, and
// never the other way round. `rename to`/`copy to` name the destination
// outright, `+++` is the new side, `---` the old side (a fallback for a diff
// that never reaches its `+++`), `diff --git`'s b-side is the last resort
// because a path with a space makes that line ambiguous.
const RANK_NONE = -1
const RANK_DIFF_GIT = 0
const RANK_OLD = 1
const RANK_NEW = 2
const RANK_RENAME = 3

interface Building extends DiffFile {
  pathRank: number
}

function blank(path = ``, status: DiffStatus = `modified`): Building {
  return {
    path,
    status,
    additions: 0,
    deletions: 0,
    binary: false,
    hunks: [],
    pathRank: RANK_NONE,
  }
}

function seal(file: Building): DiffFile {
  const { pathRank: _rank, ...rest } = file
  return rest
}

function clamp(n: number): number {
  if (!Number.isFinite(n) || n <= 0) return 0
  return n > DIFF_LINE_MAX ? DIFF_LINE_MAX : Math.floor(n)
}

/** A digit run → a counter, saturating at `DIFF_LINE_MAX`. A run too long
 *  for a double (`Number` gives Infinity) is by definition past the ceiling,
 *  never zero. */
function clampDigits(digits: string): number {
  const n = Number(digits)
  return Number.isFinite(n) ? clamp(n) : DIFF_LINE_MAX
}

/** Advance a 1-based line counter, saturating rather than wrapping. */
function step(n: number): number {
  return n >= DIFF_LINE_MAX ? DIFF_LINE_MAX : n + 1
}

/** A `---`/`+++`/`diff --git` payload → a display path: drop ONE trailing
 *  `\r` (a CRLF-framed patch, split on `\n` alone), cut at the first TAB
 *  (GNU diff's timestamp column), then unwrap surrounding double quotes (git
 *  quotes a path carrying control or non-ASCII bytes). The `a/`/`b/` prefix
 *  is stripped by `stripAb` — only where git actually writes one. */
function cutPath(raw: string): string {
  const line = raw.endsWith(`\r`) ? raw.slice(0, -1) : raw
  const tab = line.indexOf(`\t`)
  let s = tab >= 0 ? line.slice(0, tab) : line
  if (s.length >= 2 && s.startsWith(`"`) && s.endsWith(`"`)) s = s.slice(1, -1)
  return s
}

/** Drop the one `a/`/`b/` prefix git puts on `---`, `+++` and `diff --git`
 *  paths. NOT applied to `rename from`/`rename to`/`copy from`/`copy to`,
 *  which git writes bare — stripping there would eat a real top-level `a/`
 *  directory. */
function stripAb(s: string): string {
  return s.startsWith(`a/`) || s.startsWith(`b/`) ? s.slice(2) : s
}

/** The b-side of a `diff --git <a> <b>` line. Quoted pairs parse exactly;
 *  otherwise the last ` b/` wins (git's own ambiguity — an unquoted path with
 *  a space cannot be split reliably, which is why this is the lowest rank). */
function diffGitNewPath(rest: string): string | null {
  const quoted = rest.match(/^(?:"(?:[^"]*)"|\S+)\s+"([^"]*)"$/)
  if (quoted) return stripAb(quoted[1])
  const at = rest.lastIndexOf(` b/`)
  if (at >= 0) return rest.slice(at + 3)
  const trimmed = rest.trim()
  return trimmed ? stripAb(trimmed) : null
}

function parseHunkHeader(line: string): {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
} | null {
  const m = HUNK_HEADER.exec(line)
  if (!m) return null
  return {
    oldStart: clampDigits(m[1]),
    // A count the header omits is 1 — `@@ -1 +1 @@` is one line each side.
    oldLines: m[2] === undefined ? 1 : clampDigits(m[2]),
    newStart: clampDigits(m[3]),
    newLines: m[4] === undefined ? 1 : clampDigits(m[4]),
  }
}

// ── The parser ──────────────────────────────────────────────────────────────

/**
 * The one state machine behind `parseDiff` and `parsePatch`.
 *
 * `seed` = the caller already knows the path and status (a GitHub patch), so
 * every header line is ignored (except the binary marker) and no `---`/
 * `diff --git` line may ever open a second file.
 */
function parseSections(
  text: string,
  seed?: { path: string; status: DiffStatus }
): DiffFile[] {
  const files: DiffFile[] = []
  let cur: Building | null = seed ? blank(seed.path, seed.status) : null
  let hunk: DiffHunk | null = null
  let remOld = 0
  let remNew = 0
  let oldNo = 0
  let newNo = 0

  const open = (next: Building): void => {
    if (cur) files.push(seal(cur))
    cur = next
    hunk = null
    remOld = 0
    remNew = 0
  }
  /** A `@@` line, and a new file, always end the hunk in progress. */
  const closeHunk = (): void => {
    hunk = null
    remOld = 0
    remNew = 0
  }
  const setPath = (file: Building, path: string, rank: number): void => {
    if (rank >= file.pathRank) {
      file.path = path
      file.pathRank = rank
    }
  }

  const lines = text.split(`\n`)
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i]
    // 1. `diff --git` starts the next file unconditionally — even mid-hunk,
    //    where a truncated patch can leave us.
    if (!seed && raw.startsWith(`diff --git `)) {
      const file = blank()
      const path = diffGitNewPath(raw.slice(`diff --git `.length))
      if (path !== null) setPath(file, path, RANK_DIFF_GIT)
      open(file)
      continue
    }

    // 2. A hunk header. Body lines always carry a sign, so a line literally
    //    starting with `@@` is unambiguous.
    if (raw.startsWith(`@@`)) {
      closeHunk()
      const head = parseHunkHeader(raw)
      if (!head) continue
      if (!cur) cur = blank()
      hunk = { ...head, header: raw, lines: [] }
      cur.hunks.push(hunk)
      remOld = head.oldLines
      remNew = head.newLines
      oldNo = head.oldStart
      newNo = head.newStart
      continue
    }

    // 3. `\ No newline at end of file`: unified-diff metadata. Kept as a row
    //    (a reader wants to see it), numbered on neither side, and it never
    //    consumes a count — so it may legally trail a hunk whose counts are
    //    already spent, as it does when BOTH sides lack the final newline.
    if (hunk && raw.startsWith(`\\`)) {
      hunk.lines.push({
        kind: `meta`,
        text: raw.startsWith(`\\ `) ? raw.slice(2) : raw.slice(1),
      })
      continue
    }

    // 4. The hunk body, bounded by the header's counts. Past them the hunk is
    //    over, whatever the next line looks like — that is what lets a bare
    //    steer diff start its next file on a plain `--- a/…`. Inside them a
    //    `--- ` line with a `+++ ` line right behind it (one-line lookahead)
    //    is STILL the next bare section's opener, not a deletion of `-- …`: a
    //    header whose counts overshoot its body must not swallow the file
    //    after it. A `---` body line followed by anything else stays a
    //    deletion.
    const bareOpener =
      !seed &&
      raw.startsWith(`--- `) &&
      i + 1 < lines.length &&
      lines[i + 1].startsWith(`+++ `)
    if (hunk && (remOld > 0 || remNew > 0) && !bareOpener) {
      const sign = raw.charAt(0)
      if (sign === `+`) {
        cur!.additions += 1
        hunk.lines.push({ kind: `add`, newNo, text: raw.slice(1) })
        newNo = step(newNo)
        remNew -= 1
        continue
      }
      if (sign === `-`) {
        cur!.deletions += 1
        hunk.lines.push({ kind: `del`, oldNo, text: raw.slice(1) })
        oldNo = step(oldNo)
        remOld -= 1
        continue
      }
      // A context line is ` ` + content; a producer that trimmed trailing
      // whitespace emits the empty string for an empty context line, and
      // inside a hunk with counts left that is exactly what it means.
      if (sign === ` ` || raw === ``) {
        hunk.lines.push({
          kind: `context`,
          oldNo,
          newNo,
          text: raw === `` ? `` : raw.slice(1),
        })
        oldNo = step(oldNo)
        newNo = step(newNo)
        remOld -= 1
        remNew -= 1
        continue
      }
      // Anything else inside a hunk means the counts lied: end the hunk and
      // let the line be read as a header below.
      closeHunk()
    }

    if (seed) {
      if (
        cur!.hunks.length === 0 &&
        (raw.startsWith(`Binary files `) || raw.startsWith(`GIT binary patch`))
      ) {
        cur!.binary = true
      }
      continue
    }

    // 5. The header region. `---` is the one header line that may also OPEN a
    //    file: a bare steer section has no `diff --git` to announce it.
    if (raw.startsWith(`--- `) || raw === `---`) {
      if (!cur || cur.hunks.length > 0) open(blank())
      const payload = cutPath(raw.length > 4 ? raw.slice(4) : ``)
      if (payload === `/dev/null`) cur!.status = `added`
      else if (payload) setPath(cur!, stripAb(payload), RANK_OLD)
      continue
    }
    if (!cur) continue
    // Everything below is honoured only BEFORE the first hunk of a file —
    // past it these words are just content that lost its sign.
    if (cur.hunks.length > 0) continue
    if (raw.startsWith(`+++ `)) {
      const payload = cutPath(raw.slice(4))
      if (payload === `/dev/null`) cur.status = `removed`
      else if (payload) setPath(cur, stripAb(payload), RANK_NEW)
    } else if (raw.startsWith(`new file mode`)) {
      cur.status = `added`
    } else if (raw.startsWith(`deleted file mode`)) {
      cur.status = `removed`
    } else if (raw.startsWith(`rename from `)) {
      cur.previousPath = cutPath(raw.slice(`rename from `.length))
      cur.status = `renamed`
    } else if (raw.startsWith(`rename to `)) {
      setPath(cur, cutPath(raw.slice(`rename to `.length)), RANK_RENAME)
      cur.status = `renamed`
    } else if (raw.startsWith(`copy from `)) {
      cur.previousPath = cutPath(raw.slice(`copy from `.length))
      cur.status = `copied`
    } else if (raw.startsWith(`copy to `)) {
      setPath(cur, cutPath(raw.slice(`copy to `.length)), RANK_RENAME)
      cur.status = `copied`
    } else if (
      raw.startsWith(`Binary files `) ||
      raw.startsWith(`GIT binary patch`)
    ) {
      cur.binary = true
    }
  }

  if (cur) files.push(seal(cur))
  return files
}

/**
 * Read any of the three forms into the model, auto-detected:
 *
 * - a full `git diff` (`diff --git` sections),
 * - bare steer sections that start straight at `--- a/x` / `+++ b/x`,
 * - hunks-only text (the first non-blank line is a `@@` header) → ONE file
 *   with an EMPTY path and status `modified`; a caller that knows the path
 *   uses `parsePatch` instead.
 *
 * Garbage, whitespace and the empty string all yield `{ files: [] }`.
 */
export function parseDiff(text: string): Diff {
  if (!text) return { files: [] }
  let body = text
  let truncatedLines: number | undefined
  const cut = TRUNCATION_MARKER.exec(body)
  if (cut) {
    body = body.slice(0, cut.index)
    truncatedLines = clampDigits(cut[1])
  }
  const files = parseSections(body).filter(
    // A section that named neither a path nor a hunk is noise, not a file.
    (file) => file.path !== `` || file.hunks.length > 0 || file.binary
  )
  if (files.length === 0) return { files: [] }
  return truncatedLines === undefined ? { files } : { files, truncatedLines }
}

/**
 * A hunks-only patch whose path and status the CALLER knows (GitHub's
 * PullFile, the desktop's per-file `git diff` wrappers). Nothing in `patch`
 * may change either one. A missing or empty patch is a file with no hunks —
 * binary, too large for GitHub to send, or a pure rename.
 */
export function parsePatch(
  path: string,
  status: DiffStatus,
  patch: string | null | undefined
): DiffFile {
  if (!patch) {
    return {
      path,
      status,
      additions: 0,
      deletions: 0,
      binary: false,
      hunks: [],
    }
  }
  return parseSections(patch, { path, status })[0]
}

/** GitHub's file status vocabulary → ours. `changed`, `unchanged` and
 *  anything a future API adds read as `modified`. */
export function pullFileStatus(status: string): DiffStatus {
  switch (status) {
    case `added`:
    case `removed`:
    case `modified`:
    case `renamed`:
    case `copied`:
      return status
    default:
      return `modified`
  }
}

/** One GitHub PullFile → one `DiffFile`. When the patch carries no hunks
 *  (absent, empty, or a pure rename) GitHub's own additions/deletions are
 *  kept — they are the only counts there are. */
export function fromPullFile(file: {
  filename: string
  previous_filename?: string | null
  status: string
  additions: number
  deletions: number
  patch?: string | null
}): DiffFile {
  const parsed = parsePatch(
    file.filename,
    pullFileStatus(file.status),
    file.patch
  )
  if (file.previous_filename) parsed.previousPath = file.previous_filename
  if (parsed.hunks.length === 0) {
    parsed.additions = clamp(file.additions)
    parsed.deletions = clamp(file.deletions)
  }
  return parsed
}

// ── Derivations ─────────────────────────────────────────────────────────────

export interface DiffTotals {
  files: number
  additions: number
  deletions: number
}

export function totals(files: readonly DiffFile[]): DiffTotals {
  let additions = 0
  let deletions = 0
  for (const file of files) {
    additions += file.additions
    deletions += file.deletions
  }
  return { files: files.length, additions, deletions }
}

/**
 * Fold sections that name the SAME path into one file, in order of first
 * appearance. A publisher may emit one section per edit, so the same file
 * arrives several times in one transcript; the reader wants one card.
 * Hunks concatenate in arrival order, counts sum, and the LATER section's
 * status, binary flag and (when it has one) previousPath win.
 */
export function mergeFilesByPath(files: readonly DiffFile[]): DiffFile[] {
  const out: DiffFile[] = []
  const at = new Map<string, number>()
  for (const file of files) {
    const seen = at.get(file.path)
    if (seen === undefined) {
      at.set(file.path, out.length)
      out.push({ ...file, hunks: [...file.hunks] })
      continue
    }
    const target = out[seen]
    // The first clone above gave the target a private array — push into it
    // (amortised O(1)) instead of copying the whole accumulation per merge.
    // ONE hunk per push: a spread `push(...file.hunks)` passes every hunk as
    // an ARGUMENT, and a huge diff overflows the engine's argument limit.
    for (const hunk of file.hunks) target.hunks.push(hunk)
    target.additions += file.additions
    target.deletions += file.deletions
    target.status = file.status
    target.binary = file.binary
    if (file.previousPath) target.previousPath = file.previousPath
  }
  return out
}

/** Unchanged lines above a file's FIRST hunk — the count a "show more"
 *  affordance offers to expand. */
export function unchangedBefore(first: DiffHunk): number {
  return Math.max(0, first.newStart - 1)
}

/** Unchanged lines between two consecutive hunks of one file. */
export function unchangedBetween(prev: DiffHunk, next: DiffHunk): number {
  return Math.max(0, next.newStart - (prev.newStart + prev.newLines))
}

export function unchangedLabel(n: number): string {
  return `${n} unchanged ${n === 1 ? `line` : `lines`}`
}

export function additionsLabel(n: number): string {
  return `+${n}`
}

/** U+2212 MINUS SIGN, not a hyphen: the deletion count sits beside `+n` in a
 *  proportional font and a hyphen reads a full notch lighter. */
export function deletionsLabel(n: number): string {
  return `−${n}`
}

export function summaryLabel(
  files: number,
  additions: number,
  deletions: number
): string {
  if (files === 0) return NO_CHANGES
  return `${files} ${files === 1 ? `file` : `files`} ${additionsLabel(
    additions
  )} ${deletionsLabel(deletions)}`
}

/**
 * The byte-lock projection: one string per row, the whole `Diff` flattened.
 * `fixtures/diff/cases.json` stores exactly this, so every platform compares
 * `[String]` instead of reimplementing structural equality. Deliberately
 * ASCII (an ASCII `-` for the deletion count, unlike `deletionsLabel`) and
 * deliberately delimited (`|…|` around content) so trailing whitespace in a
 * diff line survives the round trip.
 */
export function renderDiff(diff: Diff): string[] {
  const out: string[] = []
  if (diff.truncatedLines !== undefined) {
    out.push(`truncated ${diff.truncatedLines}`)
  }
  for (const file of diff.files) {
    const from = file.previousPath ? ` <- ${file.previousPath}` : ``
    const binary = file.binary ? ` binary` : ``
    out.push(
      `file ${file.status} ${file.path}${from}${binary} +${file.additions} -${file.deletions}`
    )
    for (const hunk of file.hunks) {
      out.push(
        `hunk ${hunk.oldStart},${hunk.oldLines} ${hunk.newStart},${hunk.newLines} |${hunk.header}|`
      )
      for (const line of hunk.lines) {
        const old = line.oldNo === undefined ? `-` : String(line.oldNo)
        const next = line.newNo === undefined ? `-` : String(line.newNo)
        const tag =
          line.kind === `context`
            ? `ctx`
            : line.kind === `add`
              ? `add`
              : line.kind === `del`
                ? `del`
                : `meta`
        out.push(`${tag} ${old} ${next} |${line.text}|`)
      }
    }
  }
  return out
}
