import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Sparkles } from "lucide-react"
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

export function CreateTeamDialog({
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
  // Plan-cap failures (free-tier owned-team cap) render as a softer nudge.
  const [limitError, setLimitError] = useState<string | null>(null)

  const reset = () => {
    setName(``)
    setError(null)
    setLimitError(null)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSubmitting(true)
    setError(null)
    setLimitError(null)
    try {
      const result = await trpc.teams.create.mutate(
        { name: name.trim() },
        // Both error cases render inline — the global mutation-error toast
        // would be redundant noise on top of them.
        { context: { skipErrorToast: true } }
      )
      const newSlug = result.team?.slug
      reset()
      onOpenChange(false)
      if (newSlug) {
        navigate({ to: `/t/$teamSlug`, params: { teamSlug: newSlug } })
      }
    } catch (e) {
      // Open to every user (EXP-188) — the only expected failure besides
      // transport errors is the free-tier owned-team cap.
      if (isPlanLimitError(e)) {
        setLimitError(e instanceof Error ? e.message : `Plan limit reached`)
      } else {
        setError(e instanceof Error ? e.message : `Failed to create team`)
      }
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
          <DialogTitle>Create team</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="team-name">Name</Label>
            <Input
              id="team-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Side Boards"
              autoFocus
            />
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {limitError && (
            <div className="flex items-start gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
              <Sparkles className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1">{limitError}</span>
            </div>
          )}
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!name.trim() || submitting}>
              {submitting ? `Creating...` : `Create team`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
