import { useState } from "react"
import { CalendarDays, X } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { releaseCollection } from "@/lib/collections"
import { formatDateForMutation } from "@/lib/domain"
import { formatDate } from "@/lib/utils"
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
import { Textarea } from "@/components/ui/textarea"
import { Calendar } from "@/components/ui/calendar"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

// "New release" dialog (EXP-56): name + optional description + optional
// target date. Mirrors create-project-dialog's structure; creation awaits the
// releases-collection txId so the caller can navigate straight to a synced
// detail page.
export function CreateReleaseDialog({
  open,
  onOpenChange,
  workspaceId,
  onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspaceId: string
  onCreated?: (releaseId: string) => void
}) {
  const [name, setName] = useState(``)
  const [description, setDescription] = useState(``)
  const [targetDate, setTargetDate] = useState<Date | undefined>(undefined)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const resetAll = () => {
    setName(``)
    setDescription(``)
    setTargetDate(undefined)
    setError(null)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed || submitting) return

    setSubmitting(true)
    setError(null)
    try {
      const { txId, release } = await trpc.releases.create.mutate({
        workspaceId,
        name: trimmed,
        description: description.trim() ? description.trim() : undefined,
        targetDate: formatDateForMutation(targetDate) ?? undefined,
      })
      await releaseCollection.utils.awaitTxId(txId)
      resetAll()
      onOpenChange(false)
      onCreated?.(release.id)
    } catch (err) {
      setError(
        err instanceof Error ? err.message : `Failed to create release`
      )
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) resetAll()
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-[26rem]">
        <DialogHeader>
          <DialogTitle>Create release</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="release-name">Name</Label>
            <Input
              id="release-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. v1.2"
              autoFocus
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="release-description">Description</Label>
            <Textarea
              id="release-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What ships in this release? (optional)"
              className="min-h-20"
            />
          </div>
          <div className="space-y-2">
            <Label>Target date</Label>
            <div className="flex items-center gap-1">
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="justify-start font-normal text-muted-foreground"
                  >
                    <CalendarDays className="size-3.5" />
                    {targetDate ? formatDate(targetDate) : `Pick a date`}
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-auto p-0" align="start">
                  <Calendar
                    mode="single"
                    selected={targetDate}
                    onSelect={setTargetDate}
                  />
                </PopoverContent>
              </Popover>
              {targetDate && (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="text-muted-foreground"
                  aria-label="Clear target date"
                  onClick={() => setTargetDate(undefined)}
                >
                  <X className="size-3.5" />
                </Button>
              )}
            </div>
          </div>

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          <DialogFooter>
            <Button type="submit" disabled={!name.trim() || submitting}>
              {submitting ? `Creating...` : `Create release`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
