import { useEffect, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { trpc } from "@/lib/trpc-client"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"

export function CreateWorkspaceDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const [name, setName] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
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

  const reset = () => {
    setName(``)
    setError(null)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSubmitting(true)
    setError(null)
    try {
      const result = await trpc.workspaces.create.mutate({
        name: name.trim(),
      })
      const newSlug = result.workspace?.slug
      reset()
      onOpenChange(false)
      if (newSlug) {
        navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug: newSlug } })
      }
    } catch (e) {
      if (isPlanLimitError(e)) {
        reset()
        onOpenChange(false)
        setUpgradeOpen(true)
      } else {
        setError(e instanceof Error ? e.message : `Failed to create workspace`)
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
          if (!next) reset()
          onOpenChange(next)
        }}
      >
        <DialogContent className="sm:max-w-[26rem]">
          <DialogHeader>
            <DialogTitle>Create workspace</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="workspace-name">Name</Label>
              <Input
                id="workspace-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Side Projects"
                autoFocus
              />
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!name.trim() || submitting}>
                {submitting ? `Creating...` : `Create workspace`}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Workspace limit reached"
        description="You've reached the maximum number of workspaces you can own on your plan. Upgrade to create more."
        proProductId={productIds.pro}
        businessProductId={productIds.business}
      />
    </>
  )
}
