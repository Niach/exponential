import { useEffect, useState } from "react"
import { ArrowLeft, Check, Github, Globe } from "lucide-react"
import type { ProjectType } from "@exp/db-schema/domain"
import { PROJECT_TYPE_OPTIONS } from "@/lib/project-types"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { derivePrefix } from "@/lib/project"
import { useCreateProject } from "@/hooks/use-create-project"

// The chosen backing repo: either an existing registry repo (by id) or a
// brand-new one picked through the GithubRepoPicker (connected inline by
// projects.create in the same transaction).
type RepoSelection =
  | { kind: `registry`; repositoryId: string; fullName: string }
  | { kind: `inline`; repo: PickerRepo }

export function CreateProjectDialog({
  open,
  onOpenChange,
  workspaceId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspaceId: string
}) {
  const { createProject } = useCreateProject()
  const [type, setType] = useState<ProjectType | null>(null)
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [submitting, setSubmitting] = useState(false)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    pro: string | null
    business: string | null
    businessYearly: string | null
  }>({ pro: null, business: null, businessYearly: null })

  const [selection, setSelection] = useState<RepoSelection | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        pro: config.creemProProductId,
        business: config.creemBusinessProductId,
        businessYearly: config.creemBusinessYearlyProductId,
      })
    })
  }, [])

  const resetAll = () => {
    setType(null)
    setName(``)
    setPrefix(``)
    setColor(`#6366f1`)
    setSelection(null)
    setError(null)
  }

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const handlePickerSelect = (repo: PickerRepo) => {
    setSelection({ kind: `inline`, repo })
  }

  // A dev board needs a repo; task/feedback boards don't.
  const needsRepo = type === `dev`
  const canSubmit =
    Boolean(name.trim()) &&
    Boolean(prefix.trim()) &&
    (!needsRepo || selection !== null)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!type || !name.trim() || !prefix.trim()) return
    if (needsRepo && !selection) return

    setSubmitting(true)
    setError(null)
    const repository = !selection
      ? undefined
      : selection.kind === `registry`
        ? { repositoryId: selection.repositoryId }
        : {
            fullName: selection.repo.fullName,
            defaultBranch: selection.repo.defaultBranch,
            private: selection.repo.private,
          }
    const result = await createProject({
      workspaceId,
      name,
      prefix,
      color,
      type,
      repository,
    })
    setSubmitting(false)
    if (result.ok) {
      resetAll()
      onOpenChange(false)
      return
    }
    if (result.error.kind === `planLimit`) {
      onOpenChange(false)
      setUpgradeOpen(true)
    } else {
      setError(result.error.message)
    }
  }

  const selectedInlineName =
    selection?.kind === `inline` ? selection.repo.fullName : null

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) resetAll()
          onOpenChange(next)
        }}
      >
        <DialogContent className="sm:max-w-[26rem]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {type !== null && (
                <Button
                  type="button"
                  variant="ghost"
                  className="h-6 w-6 p-0"
                  onClick={() => setType(null)}
                  aria-label="Back to project type"
                >
                  <ArrowLeft className="h-4 w-4" />
                </Button>
              )}
              Create project
            </DialogTitle>
          </DialogHeader>

          {type === null ? (
            <div className="space-y-2">
              {PROJECT_TYPE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => setType(option.value)}
                  className="flex w-full items-start gap-3 rounded-lg border border-border p-3 text-left transition-colors hover:border-primary/60 hover:bg-accent/40"
                >
                  <option.icon className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0">
                    <span className="flex items-center gap-1.5 text-sm font-medium">
                      {option.label}
                      {option.value === `feedback` && (
                        <Globe className="h-3.5 w-3.5 text-muted-foreground" />
                      )}
                    </span>
                    <span className="block text-xs text-muted-foreground">
                      {option.description}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="project-name">Name</Label>
              <Input
                id="project-name"
                value={name}
                onChange={(e) => handleNameChange(e.target.value)}
                placeholder="e.g. Backend API"
                autoFocus
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="project-prefix">Prefix</Label>
              <Input
                id="project-prefix"
                value={prefix}
                onChange={(e) => setPrefix(e.target.value.toUpperCase())}
                placeholder="e.g. API"
                maxLength={10}
              />
            </div>
            <div className="space-y-2">
              <Label>Color</Label>
              <ColorSwatchGrid value={color} onChange={setColor} />
            </div>

            {needsRepo && (
            <div className="space-y-2">
              <Label>Repository</Label>
              <ConnectedRepoPicker
                workspaceId={workspaceId}
                value={
                  selection?.kind === `registry` ? selection.repositoryId : null
                }
                onSelectRegistry={(repo) =>
                  setSelection({
                    kind: `registry`,
                    repositoryId: repo.id,
                    fullName: repo.fullName,
                  })
                }
                onConnectNew={handlePickerSelect}
                appendedRow={
                  selectedInlineName ? (
                    <div className="flex w-full items-center gap-2 px-3 py-2 text-sm">
                      <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                      <span className="truncate">{selectedInlineName}</span>
                      <Check className="ml-auto h-4 w-4 shrink-0 text-primary" />
                    </div>
                  ) : undefined
                }
              />
            </div>
            )}

            {type === `feedback` && (
              <p className="rounded-md border border-border bg-accent/30 px-3 py-2 text-xs text-muted-foreground">
                Feedback boards are public: issues, comments and @mentions in
                them are visible to anyone with the link. The workspace name is
                shown on the board.
              </p>
            )}

            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}

            <DialogFooter>
              <Button type="submit" disabled={!canSubmit || submitting}>
                {submitting ? `Creating...` : `Create project`}
              </Button>
            </DialogFooter>
          </form>
          )}
        </DialogContent>
      </Dialog>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Project limit reached"
        description="You've reached the maximum number of projects for your plan. Upgrade to create more."
        proProductId={productIds.pro}
        businessProductId={productIds.business}
        businessYearlyProductId={productIds.businessYearly}
        workspaceId={workspaceId}
      />
    </>
  )
}
