import { useEffect, useState } from "react"
import { Check, Github, Globe } from "lucide-react"
import type { ProjectIcon } from "@exp/db-schema/domain"
import type { Workspace } from "@/db/schema"
import { PROJECT_TEMPLATES, type ProjectTemplate } from "@/lib/project-types"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
  OWNER_ONLY_PUBLIC_HINT,
  ProjectIconColorFields,
  ProjectNameField,
  ProjectPrefixField,
  ProjectPublicSection,
} from "@/components/project-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { derivePrefix } from "@/lib/project"
import { useCreateProject } from "@/hooks/use-create-project"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"

// The chosen backing repo: either an existing registry repo (by id) or a
// brand-new one picked through the GithubRepoPicker (connected inline by
// projects.create in the same transaction).
type RepoSelection =
  | { kind: `registry`; repositoryId: string; fullName: string }
  | { kind: `inline`; repo: PickerRepo }

export function CreateProjectDialog({
  open,
  onOpenChange,
  workspace,
  initialTemplate,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspace: Workspace
  // Preselect this quickstart on open (EXP-141 — the getting-started CTAs
  // preset e.g. `feedback`).
  initialTemplate?: ProjectTemplate[`key`]
}) {
  const workspaceId = workspace.id
  const { createProject } = useCreateProject()
  // Public boards are owner-only on the server (assertWorkspaceOwner in
  // projects.create) — non-owners get the option disabled with a hint.
  const { isOwner } = useWorkspacePermissions(workspace)
  // Quickstart selection (EXP-160: no more two-step wizard — the templates
  // are a preset row above the always-visible form). Purely a visual marker
  // + preset applicator: picking one sets icon/isPublic/showRepo and never
  // touches name/prefix/color or the repo selection.
  const [selectedTemplate, setSelectedTemplate] =
    useState<ProjectTemplate | null>(null)
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
    setSelectedTemplate(null)
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
    setSelectedTemplate(next)
    setIcon(next.defaults.icon)
    setIsPublic(next.defaults.isPublic)
    setShowRepo(next.defaults.suggestsRepo)
  }

  // Preset template: applied on open (the close path resets via resetAll, so
  // re-opening with the prop preselects the quickstart again).
  useEffect(() => {
    if (!open || !initialTemplate) return
    const preset = PROJECT_TEMPLATES.find(
      (option) => option.key === initialTemplate
    )
    if (preset) applyTemplate(preset)
    // applyTemplate only fans out to state setters — safe to omit.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initialTemplate])

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
    if (!name.trim() || !prefix.trim()) return

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
      // Clamped in case membership loaded after a public template was picked
      // — the server would reject a non-owner's isPublic anyway.
      isPublic: isOwner && isPublic,
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
        <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-[26rem]">
          <DialogHeader>
            <DialogTitle>Create project</DialogTitle>
          </DialogHeader>

          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label>Quickstart</Label>
              <div className="grid grid-cols-3 gap-2">
                {PROJECT_TEMPLATES.map((option) => {
                  const ownerLocked = option.defaults.isPublic && !isOwner
                  const selected = selectedTemplate?.key === option.key
                  return (
                    <button
                      key={option.key}
                      type="button"
                      disabled={ownerLocked}
                      title={ownerLocked ? OWNER_ONLY_PUBLIC_HINT : option.description}
                      onClick={() => applyTemplate(option)}
                      className={`flex flex-col items-center gap-1.5 rounded-lg border p-3 text-center transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                        selected
                          ? `border-primary bg-accent/40`
                          : `border-border hover:border-primary/60 hover:bg-accent/40 disabled:hover:border-border disabled:hover:bg-transparent`
                      }`}
                    >
                      <option.icon className="h-5 w-5 text-muted-foreground" />
                      <span className="flex items-center gap-1 text-xs font-medium">
                        {option.label}
                        {option.defaults.isPublic && (
                          <Globe className="h-3 w-3 text-muted-foreground" />
                        )}
                      </span>
                    </button>
                  )
                })}
              </div>
              <p className="text-xs text-muted-foreground">
                {selectedTemplate
                  ? selectedTemplate.description
                  : `Optional presets — every project has the same shape.`}
              </p>
            </div>

            <ProjectNameField
              value={name}
              onChange={handleNameChange}
              autoFocus
            />
            <ProjectPrefixField value={prefix} onChange={setPrefix} />
            <ProjectIconColorFields
              icon={icon}
              onIconChange={setIcon}
              color={color}
              onColorChange={setColor}
            />

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

            <ProjectPublicSection
              checked={isPublic}
              onCheckedChange={setIsPublic}
              disabled={!isOwner}
              hint={isOwner ? undefined : OWNER_ONLY_PUBLIC_HINT}
              showWarning={isPublic}
            />

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
