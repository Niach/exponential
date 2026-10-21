import { useEffect, useState } from "react"
import type { TriggerWhen } from "@exp/db-schema/domain"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogCancel,
  FEEDBACK_WORDS,
} from "@exp/ui"
import {
  TriggerWhenFields,
  draftFromTrigger,
  draftToTrigger,
  emptyTriggerDraft,
  validateTriggerDraft,
  type TriggerDraft,
  type TriggerDraftError,
} from "@/components/trigger-fields"

// The "New trigger" / "Edit trigger" form (EXP-583). EXP-1293: a trigger
// lives on a team AGENT, which already owns its device and account, so the
// form edits the WHEN alone (NEW-2: THE schedule picker's rows, stored as a
// cron + zone, or an event name). Saving
// hands the when-part back; the agent page owns the whole `triggers` array
// and how it persists (create mode keeps it local, an existing agent writes
// `agents.update`). Invalid input = an inline error; a failed save keeps the
// dialog open, the caller toasts.

export function TriggerDialog({
  open,
  onOpenChange,
  trigger,
  onSave,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The trigger being edited; absent/null = add a new one. */
  trigger?: TriggerWhen | null
  /** Persist the when-part; a rejection keeps the dialog open. */
  onSave: (when: TriggerWhen) => Promise<void> | void
}) {
  const editing = trigger != null
  const [draft, setDraft] = useState<TriggerDraft>(emptyTriggerDraft)
  const [submitting, setSubmitting] = useState(false)
  const [errors, setErrors] = useState<TriggerDraftError[]>([])

  // Seed on OPEN only, the rule every dialog here follows.
  useEffect(() => {
    if (!open) return
    setDraft(draftFromTrigger(trigger ?? null))
    setErrors([])
    setSubmitting(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (submitting) return
    const found = validateTriggerDraft(draft)
    setErrors(found)
    if (found.length > 0) return
    setSubmitting(true)
    try {
      await onSave(draftToTrigger(draft))
      onOpenChange(false)
    } catch {
      // The caller already toasted; the dialog stays open for another try.
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* EXP-616: a bottom sheet on mobile — the body is an ordinary
          DialogBody, so it takes the fixed 94dvh detent's free height and
          scrolls inside it, content anchored to the top. */}
      <DialogContent mobile="sheet-full" className="sm:max-h-[85dvh] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? `Edit trigger` : `New trigger`}</DialogTitle>
        </DialogHeader>

        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-3">
          <DialogBody className="space-y-3">
            <TriggerWhenFields
              draft={draft}
              onChange={(next) => {
                setDraft(next)
                if (errors.length > 0) setErrors([])
              }}
              errors={errors}
            />
          </DialogBody>

          <DialogFooter>
            <DialogCancel disabled={submitting} />
            <Button
              type="submit"
              pending={submitting}
              pendingLabel={FEEDBACK_WORDS.pending.save}
            >
              {editing ? `Save changes` : `Add trigger`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
