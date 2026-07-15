import { useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { authClient } from "@/lib/auth/client"
import { useSession } from "@/hooks/use-session"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
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

// Self-service account deletion (store policy: users must be able to delete
// their account without emailing support; the native apps expose the same
// users.deleteAccount mutation). Type-your-email confirm mirrors the
// workspace-delete danger zone.
export function DeleteAccountSection() {
  const { data: session } = useSession()
  const email = session?.user?.email ?? ``
  const [showDialog, setShowDialog] = useState(false)
  const [confirmation, setConfirmation] = useState(``)
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState(``)

  const handleDelete = async () => {
    if (confirmation !== email) return
    setDeleting(true)
    setError(``)
    try {
      await trpc.users.deleteAccount.mutate({ confirm: true })
      // The users-row delete already cascaded the server session; this just
      // clears the local cookie before landing on the login page.
      await authClient.signOut().catch(() => {})
      window.location.href = `/auth/login`
    } catch (err) {
      setError(
        err instanceof Error ? err.message : `Account deletion failed`
      )
      setDeleting(false)
    }
  }

  const closeDialog = () => {
    setShowDialog(false)
    setConfirmation(``)
    setError(``)
  }

  return (
    <>
      <Card className="border-destructive/50">
        <CardHeader>
          <CardTitle className="text-base text-destructive">
            Danger Zone
          </CardTitle>
          <CardDescription>
            Permanently delete your account, including your personal teams
            and everything you created.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button variant="destructive" onClick={() => setShowDialog(true)}>
            Delete account
          </Button>
        </CardContent>
      </Card>

      <Dialog
        open={showDialog}
        onOpenChange={(open) => {
          if (!open) closeDialog()
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete your account</DialogTitle>
            <DialogDescription>
              This permanently deletes your account, your personal teams,
              and all issues and comments you created. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-2">
            <Label htmlFor="delete-account-confirm">
              Type <span className="font-semibold">{email}</span> to confirm
            </Label>
            <Input
              id="delete-account-confirm"
              value={confirmation}
              onChange={(e) => setConfirmation(e.target.value)}
              placeholder={email}
            />
            {error && <p className="text-sm text-destructive">{error}</p>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={closeDialog} disabled={deleting}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={confirmation !== email || deleting}
            >
              {deleting ? `Deleting...` : `Delete account`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
