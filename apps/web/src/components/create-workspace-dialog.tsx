import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
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

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
    .slice(0, 48)
}

export function CreateWorkspaceDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const [name, setName] = useState(``)
  const [slug, setSlug] = useState(``)
  const [slugDirty, setSlugDirty] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleNameChange = (value: string) => {
    setName(value)
    if (!slugDirty) setSlug(slugify(value))
  }

  const handleSlugChange = (value: string) => {
    setSlugDirty(true)
    setSlug(value.toLowerCase().replace(/[^a-z0-9-]/g, ``))
  }

  const reset = () => {
    setName(``)
    setSlug(``)
    setSlugDirty(false)
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
        slug: slug.trim() || undefined,
      })
      const newSlug = result.workspace?.slug
      reset()
      onOpenChange(false)
      if (newSlug) {
        navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug: newSlug } })
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to create workspace`)
    } finally {
      setSubmitting(false)
    }
  }

  return (
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
              onChange={(e) => handleNameChange(e.target.value)}
              placeholder="e.g. Side Projects"
              autoFocus
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="workspace-slug">URL slug</Label>
            <Input
              id="workspace-slug"
              value={slug}
              onChange={(e) => handleSlugChange(e.target.value)}
              placeholder="side-projects"
            />
            <p className="text-xs text-muted-foreground">
              Used in the URL: /w/{slug || `your-slug`}
            </p>
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
  )
}
