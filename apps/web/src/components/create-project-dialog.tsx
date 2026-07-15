import { useEffect, useState } from "react"
import { ArrowLeft, Check, Github, Globe } from "lucide-react"
import type { ProjectIcon } from "@exp/db-schema/domain"
import { PROJECT_TEMPLATES, type ProjectTemplate } from "@/lib/project-types"
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
import { Switch } from "@/components/ui/switch"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
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
  // Templates only pre-set the toggles below — every project has the same
  // shape (repo optional, publicness a switch).
  const [template, setTemplate] = useState<ProjectTemplate | null>(null)
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [icon, setIcon] = useState<ProjectIcon>(`code`)
  const [isPublic, setIsPublic] = useState(false)
  const [showRepo, setShowRepo] = useState(false)
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
    setTemplate(null)
    setName(``)
    setPrefix(``)
    setColor(`#6366f1`)
    setIcon(`code`)
    setIsPublic(false)
    setShowRepo(false)
    setSelection(null)
    setError(null)
  }

  const applyTemplate = (next: ProjectTemplate) => {
    setTemplate(next)
    setIcon(next.defaults.icon)
    setIsPublic(next.defaults.isPublic)
    setShowRepo(next.defaults.suggestsRepo)
  }

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const handlePickerSelect = (repo: PickerRepo) => {
    setSelection({ kind: `inline`, repo })
  }

  const canSubmit = Boolean(name.trim()) && Boolean(prefix.trim())

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!template || !name.trim() || !prefix.trim()) return

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
      icon,
      isPublic,
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
              {template !== null && (
                <Button
                  type="button"
                  variant="ghost"
                  className="h-6 w-6 p-0"
                  onClick={() => setTemplate(null)}
                  aria-label="Back to templates"
                >
                  <ArrowLeft className="h-4 w-4" />
                </Button>
              )}
              Create project
            </DialogTitle>
          </DialogHeader>

          {template === null ? (
            <div className="space-y-2">
              {PROJECT_TEMPLATES.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  onClick={() => applyTemplate(option)}
                  className="flex w-full items-start gap-3 rounded-lg border border-border p-3 text-left transition-colors hover:border-primary/60 hover:bg-accent/40"
                >
                  <option.icon className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0">
                    <span className="flex items-center gap-1.5 text-sm font-medium">
                      {option.label}
                      {option.defaults.isPublic && (
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
                // Alphanumeric only — the server floor rejects symbol
                // prefixes (EXP-46).
                onChange={(e) =>
                  setPrefix(
                    e.target.value.replace(/[^A-Za-z0-9]/g, ``).toUpperCase()
                  )
                }
                placeholder="e.g. API"
                maxLength={10}
              />
            </div>
            <div className="space-y-2">
              <Label>Icon</Label>
              <IconSwatchGrid value={icon} onChange={setIcon} color={color} />
            </div>
            <div className="space-y-2">
              <Label>Color</Label>
              <ColorSwatchGrid value={color} onChange={setColor} />
            </div>

            <div className="space-y-2">
              <Label>Repository (optional)</Label>
              {showRepo ? (
                <ConnectedRepoPicker
                  workspaceId={workspaceId}
                  value={
                    selection?.kind === `registry`
                      ? selection.repositoryId
                      : null
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
              ) : (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-full justify-start text-muted-foreground"
                  onClick={() => setShowRepo(true)}
                >
                  <Github className="mr-2 h-4 w-4" />
                  Connect a GitHub repository
                </Button>
              )}
            </div>

            <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
              <div className="min-w-0">
                <Label
                  htmlFor="project-public"
                  className="flex items-center gap-1.5 text-sm"
                >
                  <Globe className="h-3.5 w-3.5 text-muted-foreground" />
                  Public board
                </Label>
                <p className="text-xs text-muted-foreground">
                  Anyone with the link can read it.
                </p>
              </div>
              <Switch
                id="project-public"
                checked={isPublic}
                onCheckedChange={setIsPublic}
              />
            </div>

            {isPublic && (
              <p className="rounded-md border border-border bg-accent/30 px-3 py-2 text-xs text-muted-foreground">
                Public boards are readable by anyone: issues, comments and
                @mentions in them are visible to anyone with the link. The
                workspace name is shown on the board.
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
