import { useEffect, useState } from "react"
import { Button, FEEDBACK_WORDS, FieldError, Input, conceptIcon, toast } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { UpgradeDialog } from "@/components/upgrade-dialog"

// EXP-630: THE invite-by-email form — Members settings, the Linear import's
// member mapping and the "Resend invite" dialog all render this one
// component, so an invite looks and behaves the same wherever it starts.
//
// Sending creates the person's PLACEHOLDER member at once (name + email, on
// the roster, assignable) and mails the link; when the mail cannot go out the
// link is shown for sharing by hand. `placeholderUserId` turns the form into
// a re-invite of an existing placeholder (the address stays editable).
const UiMailIcon = conceptIcon(`ui-mail`)
const UiCopyIcon = conceptIcon(`ui-copy`)
const UiCheckIcon = conceptIcon(`ui-check`)
/** feedback-states.json's invalid-email input case. */
const INVALID_EMAIL = `Enter a full email address`

export interface InvitedMember {
  memberUserId: string | null
  email: string
  name: string
  token: string
  emailDelivered: boolean | null
}

export function InviteMemberForm({
  teamId,
  defaultName = ``,
  defaultEmail = ``,
  placeholderUserId,
  submitLabel = `Send invite`,
  autoFocus,
  layout = `row`,
  onInvited,
}: {
  teamId: string
  defaultName?: string
  defaultEmail?: string
  placeholderUserId?: string
  submitLabel?: string
  autoFocus?: `name` | `email`
  // `row` = name · email · button on one line (wide settings sections);
  // `stack` = one field per line (dialogs).
  layout?: `row` | `stack`
  onInvited?: (invited: InvitedMember) => void
}) {
  const [name, setName] = useState(defaultName)
  const [email, setEmail] = useState(defaultEmail)
  const [sending, setSending] = useState(false)
  // Invalid input (the address the server refused) stays under the field.
  const [emailError, setEmailError] = useState<string | null>(null)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    team: string | null
    teamYearly: string | null
  }>({ team: null, teamYearly: null })

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        team: config.creemTeamProductId,
        teamYearly: config.creemTeamYearlyProductId,
      })
    })
  }, [])

  // The server persists the address on the invite and mails the link itself.
  // Delivery is best-effort — when no transport is configured (or the send
  // fails) the link is shown so the owner can share it by hand.
  const handleSend = async () => {
    const to = email.trim()
    if (!to) return
    setSending(true)
    setEmailError(null)
    try {
      const { token, emailDelivered, memberUserId } =
        await trpc.teamInvites.create.mutate(
          {
            teamId,
            email: to,
            name: name.trim() || undefined,
            placeholderUserId,
          },
          // The plan-limit (PRECONDITION_FAILED) case opens the upgrade
          // dialog, a refused address stays under its field, the rest toasts
          // here; the global mutation-error toast would be redundant noise.
          SKIP_ERROR_TOAST
        )
      if (emailDelivered) {
        toast.success(`Invite sent to ${to}`)
        if (!placeholderUserId) {
          setName(``)
          setEmail(``)
        }
        setInviteUrl(null)
      } else {
        setInviteUrl(`${window.location.origin}/invite/${token}`)
        toast.error(
          `Couldn't email the invite. Copy the link below and share it instead.`
        )
      }
      onInvited?.({
        memberUserId,
        email: to,
        name: name.trim(),
        token,
        emailDelivered,
      })
    } catch (err) {
      if (isPlanLimitError(err)) {
        setUpgradeOpen(true)
      } else if (trpcErrorCode(err) === `BAD_REQUEST`) {
        setEmailError(trpcErrorMessage(err, INVALID_EMAIL))
      } else {
        failedAction(`send`, `the invite`, err)
      }
    } finally {
      setSending(false)
    }
  }

  return (
    <>
      <div className="space-y-2">
        <div
          className={
            layout === `stack`
              ? `flex flex-col gap-2`
              : `flex flex-col gap-2 sm:flex-row sm:items-center`
          }
        >
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name"
            autoComplete="off"
            aria-label="Invite name"
            autoFocus={autoFocus === `name`}
            className={layout === `stack` ? undefined : `sm:w-44`}
          />
          <Input
            type="email"
            value={email}
            onChange={(e) => {
              setEmail(e.target.value)
              setEmailError(null)
            }}
            placeholder="teammate@example.com"
            aria-label="Invite email address"
            aria-invalid={emailError ? true : undefined}
            autoFocus={autoFocus === `email`}
            onKeyDown={(e) => {
              if (e.key === `Enter`) void handleSend()
            }}
          />
          <Button
            className={layout === `stack` ? `self-end` : `shrink-0`}
            onClick={() => void handleSend()}
            disabled={!email.trim()}
            pending={sending}
            pendingLabel={FEEDBACK_WORDS.pending.invite}
          >
            <UiMailIcon className="mr-2 h-4 w-4" />
            {submitLabel}
          </Button>
        </div>
        <FieldError>{emailError}</FieldError>
        {inviteUrl && <InviteLinkRow url={inviteUrl} />}
      </div>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Out of seats"
        description="Everyone on your plan's seats is already in this team. Add seats to invite more teammates."
        teamProductId={productIds.team}
        teamYearlyProductId={productIds.teamYearly}
        teamId={teamId}
      />
    </>
  )
}

/** A read-only invite link with a copy button (shared with the link-invite
 * path in the Members settings). */
export function InviteLinkRow({ url }: { url: string }) {
  const [copied, setCopied] = useState(false)
  const handleCopy = async () => {
    await navigator.clipboard.writeText(url)
    setCopied(true)
    toast.success(`Invite link copied`)
    setTimeout(() => setCopied(false), 2000)
  }
  return (
    <div className="flex items-center gap-2">
      <Input
        value={url}
        readOnly
        className="text-xs font-mono"
        data-testid="invite-url-input"
      />
      <Button
        variant="outline"
        size="icon"
        onClick={() => void handleCopy()}
        className="shrink-0"
        aria-label="Copy invite URL"
      >
        {copied ? (
          <UiCheckIcon className="h-4 w-4" />
        ) : (
          <UiCopyIcon className="h-4 w-4" />
        )}
      </Button>
    </div>
  )
}
