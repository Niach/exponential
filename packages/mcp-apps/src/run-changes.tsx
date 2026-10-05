import { useState } from "react"
import { totals, type DiffFile } from "@exp/domain-contract/diff"
import { Button, DiffCounts, FileDiffList, GlassSectionHeader, conceptIcon } from "@exp/ui"

const MergeIcon = conceptIcon(`pr-merged`)

// EXP-1183 — the run face's REVIEW half: the PR's files off
// `exponential_issues_pr_files` through the real `FileDiffList` (cards only,
// every file closed until clicked; the Guide's file rows jump here), the
// `+N −M` on the band and a Merge that confirms inline before
// `exponential_pr_merge`.
export function RunChanges({
  files,
  loading,
  error,
  focusPath,
  onMerge,
}: {
  files: readonly DiffFile[]
  loading: boolean
  error: string | null
  focusPath: string | null
  /** Present only while the PR is open; resolves to an error text or null. */
  onMerge?: () => Promise<string | null>
}) {
  const sum = totals(files)
  const [confirming, setConfirming] = useState(false)
  const [merging, setMerging] = useState(false)
  const [mergeError, setMergeError] = useState<string | null>(null)

  const merge = async () => {
    if (!onMerge) return
    setMerging(true)
    setMergeError(await onMerge())
    setMerging(false)
    setConfirming(false)
  }

  const mergeControl = onMerge ? (
    confirming ? (
      <span className="ml-2 flex items-center gap-1">
        <Button variant="ghost" size="xs" disabled={merging} onClick={() => setConfirming(false)}>
          Cancel
        </Button>
        <Button variant="glass" size="xs" disabled={merging} onClick={() => void merge()}>
          <MergeIcon />
          {merging ? `Merging…` : `Squash and merge`}
        </Button>
      </span>
    ) : (
      <Button variant="ghost" size="xs" className="ml-2" onClick={() => setConfirming(true)}>
        <MergeIcon />
        Merge
      </Button>
    )
  ) : null

  return (
    <section className="flex flex-col px-7 md:px-9" data-testid="run-changes">
      <GlassSectionHeader
        label="Changes"
        className="-mx-3 w-auto"
        trailing={
          <span className="flex items-center">
            {files.length > 0 && (
              <span className="text-xs text-muted-foreground">
                {`${sum.files} ${sum.files === 1 ? `file` : `files`} `}
                <DiffCounts additions={sum.additions} deletions={sum.deletions} />
              </span>
            )}
            {mergeControl}
          </span>
        }
      />
      {mergeError && <p className="pt-2 text-xs text-destructive">{mergeError}</p>}
      {error ? (
        <p className="py-2 text-xs text-destructive">{error}</p>
      ) : loading && files.length === 0 ? (
        <p className="py-2 text-xs text-muted-foreground">Loading the pull request…</p>
      ) : (
        <FileDiffList
          files={files}
          nav="none"
          defaultCollapsed
          density="compact"
          focusPath={focusPath}
          emptyLabel="No changed files."
          className="px-0"
        />
      )}
    </section>
  )
}
