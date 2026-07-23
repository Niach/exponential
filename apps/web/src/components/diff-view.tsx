import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { common, createLowlight } from "lowlight"
import { ChevronRight, Loader2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"

export interface PullFile {
  filename: string
  status: string
  additions: number
  deletions: number
  patch?: string
}

// ---------------------------------------------------------------------------
// Tuning (web analog of the desktop IDE's virtualized diff — the web view
// caps + expands instead of virtualizing, so huge diffs never hard-freeze
// the tab).
// ---------------------------------------------------------------------------

// A file whose patch exceeds this many lines starts collapsed.
const COLLAPSE_THRESHOLD = 300
// Expanded files render this many lines per "Show more" step.
const LINE_CHUNK = 500
// Per-line syntax highlighting is skipped for patches beyond this size.
const HIGHLIGHT_LIMIT = 1500

// ---------------------------------------------------------------------------
// Syntax highlighting — reuses `lowlight` (already a dependency for the
// tiptap code blocks) with the same `common` grammar set. Token colors come
// from the shared --hljs-* theme variables via the `.diff-code` scope in
// styles.css. Highlighting is per diff line, which is cheap and good enough
// for diffs (multi-line constructs may color slightly off at hunk edges).
// ---------------------------------------------------------------------------

const lowlight = createLowlight(common)

// Filename extension → highlight.js grammar (only names present in `common`).
const EXT_TO_LANG: Record<string, string> = {
  bash: `bash`,
  sh: `bash`,
  zsh: `bash`,
  c: `c`,
  h: `c`,
  cc: `cpp`,
  cpp: `cpp`,
  cxx: `cpp`,
  hpp: `cpp`,
  cs: `csharp`,
  css: `css`,
  go: `go`,
  gql: `graphql`,
  graphql: `graphql`,
  htm: `xml`,
  html: `xml`,
  svg: `xml`,
  xml: `xml`,
  ini: `ini`,
  toml: `ini`,
  java: `java`,
  cjs: `javascript`,
  js: `javascript`,
  jsx: `javascript`,
  mjs: `javascript`,
  json: `json`,
  kt: `kotlin`,
  kts: `kotlin`,
  less: `less`,
  lua: `lua`,
  m: `objectivec`,
  md: `markdown`,
  markdown: `markdown`,
  pl: `perl`,
  php: `php`,
  py: `python`,
  r: `r`,
  rb: `ruby`,
  rs: `rust`,
  scss: `scss`,
  sql: `sql`,
  swift: `swift`,
  cts: `typescript`,
  mts: `typescript`,
  ts: `typescript`,
  tsx: `typescript`,
  vb: `vbnet`,
  yaml: `yaml`,
  yml: `yaml`,
}

function languageFor(filename: string): string | null {
  const base = filename.split(`/`).pop() ?? filename
  if (/^makefile$/i.test(base)) return `makefile`
  const dot = base.lastIndexOf(`.`)
  const ext = dot > 0 ? base.slice(dot + 1).toLowerCase() : null
  const lang = ext ? EXT_TO_LANG[ext] : null
  return lang && lowlight.registered(lang) ? lang : null
}

type HastRoot = ReturnType<typeof lowlight.highlight>
type HastChild = HastRoot[`children`][number]

function hastToReact(nodes: Array<HastChild>, keyPrefix: string): ReactNode {
  return nodes.map((node, i) => {
    if (node.type === `text`) return node.value
    if (node.type === `element`) {
      const className = Array.isArray(node.properties?.className)
        ? node.properties.className.join(` `)
        : undefined
      return (
        <span key={`${keyPrefix}-${i}`} className={className}>
          {hastToReact(node.children, `${keyPrefix}-${i}`)}
        </span>
      )
    }
    return null
  })
}

function highlightLine(lang: string, text: string, key: string): ReactNode {
  if (!text) return text
  try {
    return hastToReact(lowlight.highlight(lang, text).children, key)
  } catch {
    return text
  }
}

// ---------------------------------------------------------------------------
// Unified-patch parsing — tracks old/new line numbers from @@ hunk headers so
// the gutter matches the desktop IDE (and GitHub).
// ---------------------------------------------------------------------------

type DiffLineKind = `hunk` | `add` | `del` | `context` | `meta`

interface DiffLine {
  kind: DiffLineKind
  text: string
  oldNo: number | null
  newNo: number | null
}

function parsePatch(patch: string): DiffLine[] {
  const out: DiffLine[] = []
  let oldNo = 0
  let newNo = 0
  const raw = patch.split(`\n`)
  // GitHub patches carry no trailing newline; drop a dangling tail defensively.
  if (raw.length > 0 && raw[raw.length - 1] === ``) raw.pop()
  for (const line of raw) {
    if (line.startsWith(`@@`)) {
      const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line)
      if (m) {
        oldNo = Number(m[1])
        newNo = Number(m[2])
      }
      out.push({ kind: `hunk`, text: line, oldNo: null, newNo: null })
    } else if (line.startsWith(`+`)) {
      out.push({ kind: `add`, text: line.slice(1), oldNo: null, newNo: newNo++ })
    } else if (line.startsWith(`-`)) {
      out.push({ kind: `del`, text: line.slice(1), oldNo: oldNo++, newNo: null })
    } else if (line.startsWith(`\\`)) {
      // "\ No newline at end of file"
      out.push({ kind: `meta`, text: line, oldNo: null, newNo: null })
    } else {
      out.push({
        kind: `context`,
        text: line.startsWith(` `) ? line.slice(1) : line,
        oldNo: oldNo++,
        newNo: newNo++,
      })
    }
  }
  return out
}

function patchLineCount(patch: string | undefined): number {
  if (!patch) return 0
  return patch.split(`\n`).length
}

// ---------------------------------------------------------------------------
// File status → gutter letter (desktop IDE / IDE-conventional colors).
// ---------------------------------------------------------------------------

const STATUS_META: Record<string, { label: string; className: string }> = {
  added: { label: `A`, className: `text-emerald-400` },
  removed: { label: `D`, className: `text-rose-400` },
  modified: { label: `M`, className: `text-amber-400` },
  changed: { label: `M`, className: `text-amber-400` },
  renamed: { label: `R`, className: `text-sky-400` },
  copied: { label: `C`, className: `text-sky-400` },
}

function statusMeta(status: string) {
  return STATUS_META[status] ?? { label: `M`, className: `text-amber-400` }
}

function FilePathLabel({ filename }: { filename: string }) {
  const slash = filename.lastIndexOf(`/`)
  const dir = slash >= 0 ? filename.slice(0, slash + 1) : ``
  const base = filename.slice(slash + 1)
  return (
    <span className="min-w-0 truncate font-mono">
      {dir ? <span className="text-muted-foreground">{dir}</span> : null}
      {base}
    </span>
  )
}

function AddDelCounts({
  additions,
  deletions,
}: {
  additions: number
  deletions: number
}) {
  return (
    <span className="shrink-0 font-mono">
      <span className="text-emerald-400">+{additions}</span>
      {` `}
      <span className="text-rose-400">-{deletions}</span>
    </span>
  )
}

// ---------------------------------------------------------------------------
// Per-file section — collapsible, sticky header, line-numbered gutter,
// syntax-highlighted rows, cap-with-expand for huge patches.
// ---------------------------------------------------------------------------

const GUTTER_CLASS = `select-none pr-2 text-right tabular-nums text-muted-foreground/50`

interface FilePatchProps {
  file: PullFile
  open: boolean
  onOpenChange: (open: boolean) => void
}

function FilePatch({ file, open, onOpenChange }: FilePatchProps) {
  const meta = statusMeta(file.status)
  const lines = useMemo(
    () => (file.patch ? parsePatch(file.patch) : []),
    [file.patch]
  )
  const lang = useMemo(() => languageFor(file.filename), [file.filename])

  const [visibleCount, setVisibleCount] = useState(LINE_CHUNK)
  // Reset the reveal cap when the patch itself changes (tier-3 refresh).
  useEffect(() => setVisibleCount(LINE_CHUNK), [file.patch])

  const doHighlight = lang !== null && lines.length <= HIGHLIGHT_LIMIT

  // Rows are built only while expanded — collapsed large files cost nothing.
  const rows = useMemo(() => {
    if (!open || lines.length === 0) return null
    return lines.slice(0, visibleCount).map((line, i) => {
      if (line.kind === `hunk`) {
        return (
          <div
            key={i}
            className="bg-indigo-500/5 px-3 py-0.5 whitespace-pre text-indigo-300/80"
          >
            {line.text}
          </div>
        )
      }
      if (line.kind === `meta`) {
        return (
          <div key={i} className="px-3 italic whitespace-pre text-muted-foreground/70">
            {line.text}
          </div>
        )
      }
      const content =
        doHighlight && lang
          ? highlightLine(lang, line.text, `${i}`)
          : line.text
      return (
        <div
          key={i}
          className={cn(
            `grid grid-cols-[3rem_3rem_1rem_1fr]`,
            line.kind === `add` && `bg-emerald-500/10`,
            line.kind === `del` && `bg-rose-500/10`
          )}
        >
          <span className={GUTTER_CLASS}>{line.oldNo ?? ``}</span>
          <span className={GUTTER_CLASS}>{line.newNo ?? ``}</span>
          <span
            className={cn(
              `select-none text-center`,
              line.kind === `add` && `text-emerald-400`,
              line.kind === `del` && `text-rose-400`
            )}
          >
            {line.kind === `add` ? `+` : line.kind === `del` ? `-` : ``}
          </span>
          <span
            className={cn(
              `pr-3 whitespace-pre [tab-size:4]`,
              doHighlight
                ? `text-foreground/90`
                : line.kind === `add`
                  ? `text-emerald-300`
                  : line.kind === `del`
                    ? `text-rose-300`
                    : `text-muted-foreground`
            )}
          >
            {content || ` `}
          </span>
        </div>
      )
    })
  }, [open, lines, visibleCount, lang, doHighlight])

  const hiddenCount = Math.max(0, lines.length - visibleCount)

  return (
    // overflow-clip (not hidden) keeps the rounded corners without creating a
    // scroll container, so the sticky header can stick to the tab's scrollport.
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      className="overflow-clip rounded-md border border-border"
    >
      <div className="sticky top-0 z-10 rounded-t-md bg-background">
        <CollapsibleTrigger
          className={cn(
            `flex w-full items-center gap-2 rounded-t-md border-b border-border bg-muted/30 px-3 py-1.5 text-left text-xs transition-colors hover:bg-muted/50`,
            !open && `rounded-b-md border-b-transparent`
          )}
        >
          <ChevronRight
            className={cn(
              `size-3.5 shrink-0 text-muted-foreground transition-transform`,
              open && `rotate-90`
            )}
          />
          <span
            className={cn(
              `w-3 shrink-0 text-center font-mono font-semibold`,
              meta.className
            )}
          >
            {meta.label}
          </span>
          <FilePathLabel filename={file.filename} />
          {!open && lines.length > 0 ? (
            <span className="shrink-0 text-muted-foreground">
              {lines.length} lines
            </span>
          ) : null}
          <span className="ml-auto" />
          <AddDelCounts additions={file.additions} deletions={file.deletions} />
        </CollapsibleTrigger>
      </div>
      <CollapsibleContent>
        {file.patch ? (
          <>
            <div className="diff-code overflow-x-auto font-mono text-[0.6875rem] leading-relaxed">
              <div className="w-max min-w-full py-0.5">{rows}</div>
            </div>
            {hiddenCount > 0 && (
              <Button
                variant="ghost"
                size="xs"
                className="w-full rounded-none border-t border-border text-muted-foreground"
                onClick={() => setVisibleCount((c) => c + LINE_CHUNK)}
              >
                Show {Math.min(LINE_CHUNK, hiddenCount)} more lines (
                {hiddenCount} hidden)
              </Button>
            )}
          </>
        ) : (
          <div className="px-3 py-2 text-xs text-muted-foreground">
            {file.status === `renamed`
              ? `Renamed without content changes.`
              : `No textual diff (binary or too large).`}
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}

// ---------------------------------------------------------------------------
// File navigation summary — totals + click-to-scroll list (the web analog of
// the desktop IDE's diff file list + scroll_to_file).
// ---------------------------------------------------------------------------

function FileNav({
  files,
  onJump,
}: {
  files: PullFile[]
  onJump: (filename: string) => void
}) {
  const additions = files.reduce((n, f) => n + f.additions, 0)
  const deletions = files.reduce((n, f) => n + f.deletions, 0)
  return (
    <div className="rounded-md border border-border">
      <div className="flex items-center gap-2 rounded-t-md border-b border-border bg-muted/30 px-3 py-1.5 text-xs">
        <span className="font-medium">{files.length} files changed</span>
        <span className="ml-auto" />
        <AddDelCounts additions={additions} deletions={deletions} />
      </div>
      <div className="max-h-48 overflow-y-auto py-1">
        {files.map((f) => {
          const meta = statusMeta(f.status)
          return (
            <Button
              key={f.filename}
              variant="ghost"
              size="xs"
              onClick={() => onJump(f.filename)}
              className="flex h-6 w-full items-center justify-start gap-2 rounded-none px-3 font-normal"
            >
              <span
                className={cn(
                  `w-3 shrink-0 text-center font-mono font-semibold`,
                  meta.className
                )}
              >
                {meta.label}
              </span>
              <FilePathLabel filename={f.filename} />
              <span className="ml-auto" />
              <AddDelCounts additions={f.additions} deletions={f.deletions} />
            </Button>
          )
        })}
      </div>
    </div>
  )
}

// Shared file-patch list — reused by the PR-diff tier (this file's DiffView)
// and the pushed-branch-no-PR tier (the review-detail route's BranchDiffSection),
// both of which get their files in the same `PullFile[]` shape (github-pr.ts /
// github-app.ts). The review-detail route passes `showFileNav={false}` +
// `defaultCollapsed` to match the iOS/Android review layout (EXP-248): a slim
// totals row instead of the jump-list card, every file starting closed.
export function FileDiffList({
  files,
  showFileNav = true,
  defaultCollapsed = false,
}: {
  files: PullFile[]
  showFileNav?: boolean
  defaultCollapsed?: boolean
}) {
  // Sparse user overrides on top of size-based defaults, keyed by filename —
  // a tier-3 refresh replaces `files` without discarding the user's toggles.
  const [overrides, setOverrides] = useState<Record<string, boolean>>({})
  const sectionRefs = useRef(new Map<string, HTMLDivElement>())

  const defaults = useMemo(() => {
    const map = new Map<string, boolean>()
    for (const f of files) {
      map.set(
        f.filename,
        !defaultCollapsed && patchLineCount(f.patch) <= COLLAPSE_THRESHOLD
      )
    }
    return map
  }, [files, defaultCollapsed])

  const jumpTo = (filename: string) => {
    setOverrides((prev) => ({ ...prev, [filename]: true }))
    // Scroll on the next frame so a just-expanded section has laid out.
    requestAnimationFrame(() => {
      sectionRefs.current
        .get(filename)
        ?.scrollIntoView({ behavior: `smooth`, block: `start` })
    })
  }

  const totalAdditions = files.reduce((n, f) => n + f.additions, 0)
  const totalDeletions = files.reduce((n, f) => n + f.deletions, 0)

  return (
    <div className="space-y-2 p-3">
      {showFileNav ? (
        files.length > 1 && <FileNav files={files} onJump={jumpTo} />
      ) : (
        <div className="flex items-center gap-2 px-1 text-xs">
          <span className="font-medium">
            {files.length === 1 ? `1 file changed` : `${files.length} files changed`}
          </span>
          <span className="ml-auto" />
          <AddDelCounts additions={totalAdditions} deletions={totalDeletions} />
        </div>
      )}
      {files.map((f) => (
        <div
          key={f.filename}
          ref={(el) => {
            if (el) sectionRefs.current.set(f.filename, el)
            else sectionRefs.current.delete(f.filename)
          }}
          className="scroll-mt-2"
        >
          <FilePatch
            file={f}
            open={overrides[f.filename] ?? defaults.get(f.filename) ?? true}
            onOpenChange={(open) =>
              setOverrides((prev) => ({ ...prev, [f.filename]: open }))
            }
          />
        </div>
      ))}
    </div>
  )
}

export function DiffView({
  issueId,
  showFileNav,
  defaultCollapsed,
}: {
  issueId: string
  showFileNav?: boolean
  defaultCollapsed?: boolean
}) {
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [files, setFiles] = useState<PullFile[]>([])

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    trpc.issues.prFiles
      .query({ issueId })
      .then((res) => {
        if (cancelled) return
        setFiles(res.files)
        setLoading(false)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setError(err instanceof Error ? err.message : `Failed to load changes`)
        setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [issueId])

  if (loading) {
    return (
      <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" /> Loading changes…
      </div>
    )
  }
  if (error) {
    return (
      <div className="px-3 py-3 text-xs text-rose-300">
        Couldn’t load changes: {error}
      </div>
    )
  }
  if (files.length === 0) {
    return (
      <div className="px-3 py-3 text-xs text-muted-foreground">
        No changed files.
      </div>
    )
  }

  return (
    <FileDiffList
      files={files}
      showFileNav={showFileNav}
      defaultCollapsed={defaultCollapsed}
    />
  )
}
