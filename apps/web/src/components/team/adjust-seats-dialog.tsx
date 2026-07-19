import { useEffect, useState } from "react"
import { Loader2, Minus, Plus } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"

// Self-service seat adjustment on the team's EXISTING subscription
// (billing.updateSeats). This replaces re-running checkout, which would stack
// a second full-price subscription on top of the first one. Seats apply
// immediately; the price change lands on the next renewal invoice
// (proration-none — see lib/billing/creem-subscriptions.ts).
export function AdjustSeatsDialog({
  teamId,
  currentSeats,
  memberCount,
  periodEnd,
  open,
  onOpenChange,
}: {
  teamId: string
  currentSeats: number
  memberCount: number
  periodEnd: string | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [seats, setSeats] = useState(currentSeats)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (open) setSeats(currentSeats)
  }, [open, currentSeats])

  const renewalDate = periodEnd
    ? new Date(periodEnd).toLocaleDateString(undefined, {
        year: `numeric`,
        month: `long`,
        day: `numeric`,
      })
    : null

  const belowMembers = seats < memberCount
  const changed = seats !== currentSeats

  const handleSave = async () => {
    setSaving(true)
    try {
      const result = await trpc.billing.updateSeats.mutate({
        teamId,
        seats,
      })
      invalidateBillingCache()
      toast.success(
        `Team updated to ${result.seats} seat${result.seats === 1 ? `` : `s`}`
      )
      onOpenChange(false)
    } catch (err) {
      console.error(`[billing] seat update failed:`, err)
      toast.error(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't update seats`
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Adjust seats</DialogTitle>
          <DialogDescription>
            Seat changes apply immediately.
            {renewalDate
              ? ` Your next invoice on ${renewalDate} will bill the new seat count.`
              : ` Your next invoice will bill the new seat count.`}
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center justify-between gap-2">
          <Label className="text-sm text-muted-foreground">Seats</Label>
          <div className="flex items-center gap-1">
            <Button
              type="button"
              variant="outline"
              size="icon"
              className="size-8 shrink-0"
              onClick={() => setSeats(Math.max(1, seats - 1))}
              disabled={seats <= 1 || saving}
              aria-label="Remove seat"
            >
              <Minus className="size-3.5" />
            </Button>
            <Input
              type="text"
              inputMode="numeric"
              value={seats}
              onChange={(e) => {
                const n = Number.parseInt(
                  e.target.value.replace(/\D/g, ``),
                  10
                )
                setSeats(Number.isFinite(n) && n >= 1 ? n : 1)
              }}
              className="h-8 w-14 px-1 text-center"
              aria-label="Seat count"
              disabled={saving}
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              className="size-8 shrink-0"
              onClick={() => setSeats(seats + 1)}
              disabled={saving}
              aria-label="Add seat"
            >
              <Plus className="size-3.5" />
            </Button>
          </div>
        </div>

        {belowMembers && (
          <p className="text-xs text-amber-500">
            This team has {memberCount} members. Reducing below that
            blocks new invites — existing members keep working.
          </p>
        )}

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!changed || saving}>
            {saving && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
            {saving
              ? `Saving...`
              : changed
                ? `Set to ${seats} seat${seats === 1 ? `` : `s`}`
                : `No change`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
