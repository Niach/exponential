// Raw `git diff` (unified) → per-file sections, shaped like the GitHub
// PullFile rows the diff components already render (diff-view.tsx). Used by
// the live agent-session view: the steer relay's activity channel delivers
// the desktop's latest worktree diff as ONE raw unified-diff string, and this
// splits it so FileDiffList can render it like a PR diff. Mirrors the mobile
// ports (Android ui/issue splitUnifiedDiff / iOS DiffRendering.splitFiles).

export interface UnifiedDiffFile {
  filename: string
  status: string // added | removed | modified | renamed (PullFile vocabulary)
  additions: number
  deletions: number
  /** Hunk lines from the first `@@` on; undefined for binary/no-content. */
  patch?: string
}

export interface UnifiedDiffStats {
  additions: number
  deletions: number
}

/** Strip git's optional quoting and the a/ b/ prefix from a diff path. */
function cleanPath(raw: string, prefix: `a/` | `b/`): string | null {
  let path = raw.trim()
  if (path.startsWith(`"`) && path.endsWith(`"`) && path.length >= 2) {
    path = path.slice(1, -1)
  }
  if (path === `/dev/null`) return null
  if (path.startsWith(prefix)) return path.slice(prefix.length)
  return path
}

interface Section {
  header: string[]
  body: string[]
}

function toFile(section: Section): UnifiedDiffFile {
  const { header, body } = section

  let status = `modified`
  let renameTo: string | null = null
  for (const line of header) {
    if (line.startsWith(`new file mode`)) status = `added`
    else if (line.startsWith(`deleted file mode`)) status = `removed`
    else if (line.startsWith(`rename to `)) {
      status = `renamed`
      renameTo = line.slice(`rename to `.length).trim()
    }
  }

  // Filename: rename target > `+++ b/…` > `--- a/…` > the `diff --git` line.
  let filename = renameTo
  if (!filename) {
    for (const line of header) {
      if (line.startsWith(`+++ `)) {
        filename = cleanPath(line.slice(4), `b/`)
        if (filename) break
      }
    }
  }
  if (!filename) {
    for (const line of header) {
      if (line.startsWith(`--- `)) {
        filename = cleanPath(line.slice(4), `a/`)
        if (filename) break
      }
    }
  }
  if (!filename) {
    const m = /^diff --git "?a\/(.+?)"? "?b\/(.+?)"?$/.exec(header[0] ?? ``)
    filename = m?.[2] ?? header[0] ?? ``
  }

  let additions = 0
  let deletions = 0
  for (const line of body) {
    if (line.startsWith(`+`)) additions++
    else if (line.startsWith(`-`)) deletions++
  }

  return {
    filename,
    status,
    additions,
    deletions,
    // Binary / mode-only sections have no hunks — patch stays undefined so
    // FilePatch shows its "No textual diff" row.
    patch: body.length > 0 ? body.join(`\n`) : undefined,
  }
}

/**
 * Split raw `git diff` output on `diff --git` boundaries. Everything before
 * the first `@@` of a section is header (index/---/+++/mode lines); the rest
 * is the patch body handed to the existing unified-patch renderer.
 */
export function splitUnifiedDiff(diff: string): UnifiedDiffFile[] {
  const files: UnifiedDiffFile[] = []
  let current: Section | null = null
  let inBody = false

  for (const line of diff.split(`\n`)) {
    if (line.startsWith(`diff --git `)) {
      if (current) files.push(toFile(current))
      current = { header: [line], body: [] }
      inBody = false
      continue
    }
    if (!current) continue // preamble before the first file — ignore
    if (!inBody && line.startsWith(`@@`)) inBody = true
    if (inBody) current.body.push(line)
    else current.header.push(line)
  }
  if (current) files.push(toFile(current))

  // Trailing blank line from the final `\n` split — drop it from the last
  // patch so the renderer doesn't show a phantom empty row.
  const last = files[files.length - 1]
  if (last?.patch?.endsWith(`\n`)) {
    last.patch = last.patch.slice(0, -1)
  }
  return files
}

export function unifiedDiffStats(diff: string): UnifiedDiffStats {
  let additions = 0
  let deletions = 0
  for (const file of splitUnifiedDiff(diff)) {
    additions += file.additions
    deletions += file.deletions
  }
  return { additions, deletions }
}
