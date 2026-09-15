import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { extractInviteToken } from "@/components/onboarding/wizard"
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

// EXP-870 (parity): the team menu's "Join team" — the desktop's JoinTeam
// dialog (`join_team.rs`, the sidebar team menu) on the web. Same accepted
// input as onboarding's join step (an invite link or a bare token); the
// invite page previews and accepts, so this only routes there.
export function JoinTeamDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const [link, setLink] = useState(``)
  const [error, setError] = useState<string | null>(null)

  const close = (next: boolean) => {
    if (!next) {
      setLink(``)
      setError(null)
    }
    onOpenChange(next)
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    const token = extractInviteToken(link)
    if (!token) {
      setError(
        `That doesn't look like an invite link. It should look like ` +
          `${window.location.origin}/invite/…`
      )
      return
    }
    close(false)
    void navigate({ to: `/invite/$token`, params: { token } })
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-[26rem]">
        <DialogHeader>
          <DialogTitle>Join team</DialogTitle>
        </DialogHeader>
        <form
          onSubmit={handleSubmit}
          className="flex min-h-0 flex-1 flex-col gap-4"
        >
          <DialogBody className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="join-team-link">Invite link</Label>
              <Input
                id="join-team-link"
                value={link}
                onChange={(e) => {
                  setLink(e.target.value)
                  setError(null)
                }}
                placeholder={`${window.location.origin}/invite/…`}
                autoFocus
              />
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
          </DialogBody>
          <DialogFooter>
            <DialogCancel variant="outline" onClick={() => close(false)} />
            <Button type="submit" disabled={!link.trim()}>
              Continue
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
