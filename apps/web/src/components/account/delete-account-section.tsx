import { useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { deleteAccountPrompt, promptActions } from "@/lib/prompts"
import { authClient } from "@/lib/auth/client"
import { useSession } from "@/hooks/use-session"
import {
  Button,
  GlassRow,
  GlassSectionHeader,
  Input,
  Label,
  Prompt,
} from "@exp/ui"

// Self-service account deletion (store policy: users must be able to delete
// their account without emailing support; the native apps expose the same
// users.deleteAccount mutation). Type-your-email confirm mirrors the
// team-delete danger zone.
const deleteCopy = deleteAccountPrompt()

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
      setError(err instanceof Error ? err.message : `Account deletion failed`)
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
      <div>
        <GlassSectionHeader
          label="Danger zone"
          className="[&>span]:text-destructive"
        />
        <GlassRow className="justify-between gap-3">
          <span className="min-w-0 text-sm text-muted-foreground">
            Permanently delete your account and every team where you are the
            only member.
          </span>
          <Button
            variant="destructive"
            size="xs"
            className="shrink-0"
            onClick={() => setShowDialog(true)}
          >
            Delete account
          </Button>
        </GlassRow>
      </div>

      <Prompt
        open={showDialog}
        onOpenChange={(open) => {
          if (!open) closeDialog()
        }}
        busy={deleting}
        className="sm:max-w-lg"
        title={deleteCopy.title}
        // The consequences list stands IN PLACE of the contract body (the
        // entry's web `slot`): it adds the subscription facts. Rendered AS
        // the body so the card announces it.
        // Accurate per-team consequences (REV2-55/REV2-36): a solo team is
        // destroyed WITH its paid plan, while a shared team (and the
        // subscription funding it) survives, because a subscription belongs
        // to the team and not to whoever paid for it.
        body={
          <ul className="list-disc space-y-1 pl-5">
            <li>
              Teams where you are the only member are deleted with all their
              boards, issues and files. If one of them has a paid plan, that
              subscription is cancelled immediately, with no refund for the
              rest of the period.
            </li>
            <li>
              Teams you share with others stay exactly as they are, including
              their paid plan. A subscription belongs to the team, so it keeps
              running and the remaining owners keep managing it.
            </li>
            <li>
              In shared teams, the issues you created and the images you
              uploaded stay (they are part of the team&apos;s work); your
              comments are deleted and mentions of your email address are
              anonymized.
            </li>
          </ul>
        }
        actions={promptActions(deleteCopy, {
          delete: {
            busy: deleting,
            disabled: confirmation !== email,
            onSelect: () => handleDelete(),
          },
        })}
      >
        <div className="space-y-2">
          <Label htmlFor="delete-account-confirm">
            <span>
              Type <span className="font-semibold">{email}</span> to confirm
            </span>
          </Label>
          <Input
            id="delete-account-confirm"
            data-prompt-autofocus
            value={confirmation}
            onChange={(e) => setConfirmation(e.target.value)}
            placeholder={email}
          />
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
      </Prompt>
    </>
  )
}
