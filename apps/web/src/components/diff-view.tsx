import { useEffect, useState } from "react"
import { FileDiff, Loader2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

export interface PullFile {
  filename: string
  status: string
  additions: number
  deletions: number
  patch?: string
}

function lineClass(line: string): string {
  if (line.startsWith(`@@`)) return `text-indigo-300/80 bg-indigo-500/5`
  if (line.startsWith(`+`)) return `text-emerald-300 bg-emerald-500/10`
  if (line.startsWith(`-`)) return `text-rose-300 bg-rose-500/10`
  return `text-muted-foreground`
}

function FilePatch({ file }: { file: PullFile }) {
  return (
    <div className="overflow-hidden rounded-md border border-border">
      <div className="flex items-center gap-2 border-b border-border bg-muted/30 px-3 py-1.5 text-xs">
        <FileDiff className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate font-mono">{file.filename}</span>
        <span className="ml-auto shrink-0 font-mono">
          <span className="text-emerald-400">+{file.additions}</span>{` `}
          <span className="text-rose-400">-{file.deletions}</span>
        </span>
      </div>
      {file.patch ? (
        <pre className="overflow-x-auto text-[0.6875rem] leading-relaxed">
          {file.patch.split(`\n`).map((line, i) => (
            <div key={i} className={cn(`px-3`, lineClass(line))}>
              {line || ` `}
            </div>
          ))}
        </pre>
      ) : (
        <div className="px-3 py-2 text-xs text-muted-foreground">
          {file.status === `renamed`
            ? `Renamed.`
            : `No textual diff (binary or too large).`}
        </div>
      )}
    </div>
  )
}

// Shared file-patch list — reused by the PR-diff tier (this file's DiffView)
// and the pushed-branch-no-PR tier (issue-changes-tab.tsx), both of which get
// their files in the same `PullFile[]` shape (github-pr.ts / github-app.ts).
export function FileDiffList({ files }: { files: PullFile[] }) {
  return (
    <div className="space-y-2 p-3">
      {files.map((f) => (
        <FilePatch key={f.filename} file={f} />
      ))}
    </div>
  )
}

export function DiffView({ issueId }: { issueId: string }) {
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

  return <FileDiffList files={files} />
}
