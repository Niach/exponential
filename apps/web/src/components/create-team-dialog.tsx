import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Sparkles } from "lucide-react"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import {
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Button,
  Input,
  Label,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"

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
  // Plan-cap failures (free-tier owned-team cap) render as a softer nudge.
  const [limitError, setLimitError] = useState<string | null>(null)

  const reset = () => {
    setName(``)
    setLimitError(null)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSubmitting(true)
    setLimitError(null)
    try {
      const result = await trpc.teams.create.mutate(
        { name: name.trim() },
        // The cap renders inline, anything else is this site's own toast:
        // the global mutation-error toast would double it.
        SKIP_ERROR_TOAST
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
        failedAction(`create`, `the team`, e)
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
        <form
          onSubmit={handleSubmit}
          className="flex min-h-0 flex-1 flex-col gap-4"
        >
          <DialogBody className="space-y-4">
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
            {limitError && (
              <div className="flex items-start gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
                <Sparkles className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
                <span className="min-w-0 flex-1">{limitError}</span>
              </div>
            )}
          </DialogBody>
          <DialogFooter>
            <DialogCancel
              variant="outline"
              onClick={() => onOpenChange(false)}
            />
            <Button type="submit" pending={submitting} disabled={!name.trim()}>
              Create team
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
