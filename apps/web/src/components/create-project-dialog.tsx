import { useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Check, Github } from "lucide-react"
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
import { workspaceCollection } from "@/lib/collections"
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

  const { data: workspaceRows } = useLiveQuery(
    (query) =>
      query
        .from({ workspaces: workspaceCollection })
        .where(({ workspaces }) => eq(workspaces.id, workspaceId)),
    [workspaceId]
  )
  const workspaceSlug = workspaceRows?.[0]?.slug ?? null

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

  const canSubmit =
    Boolean(name.trim()) && Boolean(prefix.trim()) && selection !== null

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || !prefix.trim() || !selection) return

    setSubmitting(true)
    setError(null)
    const repository =
      selection.kind === `registry`
        ? { repositoryId: selection.repositoryId }
        : {
            fullName: selection.repo.fullName,
            defaultBranch: selection.repo.defaultBranch,
            private: selection.repo.private,
            installationId: selection.repo.installationId,
          }
    const result = await createProject({
      workspaceId,
      name,
      prefix,
      color,
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

  // The App-absent empty state used inside the picker: point owners at
  // workspace settings → Repositories to install the GitHub App.
  const installEmptyState = (
    <div className="space-y-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
      <div className="flex items-start gap-2">
        <Github className="mt-0.5 h-4 w-4 shrink-0" />
        <span>
          No repository is connected yet. Connect the GitHub App in workspace
          settings, then pick a repository here.
        </span>
      </div>
      {workspaceSlug && (
        <Button
          asChild
          type="button"
          variant="link"
          size="sm"
          className="px-0"
        >
          <Link
            to="/w/$workspaceSlug/settings"
            params={{ workspaceSlug }}
            onClick={() => onOpenChange(false)}
          >
            Go to Repositories settings
          </Link>
        </Button>
      )}
    </div>
  )

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
            <DialogTitle>Create project</DialogTitle>
          </DialogHeader>

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
                installEmptyState={installEmptyState}
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
