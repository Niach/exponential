import { useEffect, useState } from "react"
import { ArrowLeft, Github } from "lucide-react"
import { TRPCClientError } from "@trpc/client"
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
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"
import { invalidateSetupChecklistCache } from "@/hooks/use-setup-checklist"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { derivePrefix } from "@/lib/project"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"

// "owner/my-cool-repo" -> "My Cool Repo"
function prettifyRepoName(fullName: string): string {
  const segment = fullName.split(`/`)[1] ?? fullName
  return segment
    .split(/[-_]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(` `)
}

export function CreateProjectDialog({
  open,
  onOpenChange,
  workspaceId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspaceId: string
}) {
  // Step 1 picks a repo (or skips to plain tracking); step 2 confirms details.
  const [step, setStep] = useState<`source` | `details`>(`source`)
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [repo, setRepo] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    pro: string | null
    business: string | null
  }>({ pro: null, business: null })

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        pro: config.creemProProductId,
        business: config.creemBusinessProductId,
      })
    })
  }, [])

  const resetAll = () => {
    setStep(`source`)
    setName(``)
    setPrefix(``)
    setColor(`#6366f1`)
    setRepo(null)
  }

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const handlePickRepo = (picked: PickerRepo) => {
    const derived = prettifyRepoName(picked.fullName)
    setName(derived)
    setPrefix(derivePrefix(derived))
    setRepo(picked.fullName)
    setStep(`details`)
  }

  const handleSkip = () => {
    setRepo(null)
    setStep(`details`)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || !prefix.trim()) return

    setSubmitting(true)
    try {
      await trpc.projects.create.mutate({
        workspaceId,
        name: name.trim(),
        prefix: prefix.trim(),
        color,
        repo: repo ?? undefined,
      })
      invalidateBillingCache()
      invalidateSetupChecklistCache()
      resetAll()
      onOpenChange(false)
    } catch (err) {
      if (err instanceof TRPCClientError && err.data?.code === `FORBIDDEN`) {
        onOpenChange(false)
        setUpgradeOpen(true)
      }
    } finally {
      setSubmitting(false)
    }
  }

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

          {step === `source` ? (
            <div className="space-y-3">
              <p className="text-sm text-muted-foreground">
                Connect a GitHub repo so a coding agent can work on it — or track
                without one.
              </p>
              <GithubRepoPicker onSelect={handlePickRepo} onSkip={handleSkip} />
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              {repo && (
                <div className="flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-2 text-sm">
                  <Github className="h-4 w-4 shrink-0" />
                  <span className="truncate font-medium">{repo}</span>
                </div>
              )}
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
              <DialogFooter>
                <Button
                  type="button"
                  variant="ghost"
                  onClick={() => setStep(`source`)}
                >
                  <ArrowLeft className="mr-1 h-4 w-4" />
                  Back
                </Button>
                <Button
                  type="submit"
                  disabled={!name.trim() || !prefix.trim() || submitting}
                >
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
      />
    </>
  )
}
