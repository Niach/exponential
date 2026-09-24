import { useEffect, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { Button, Input, conceptIcon } from "@exp/ui"
import { toast } from "sonner"
import { trpc } from "@/lib/trpc-client"
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
          // dialog; the global mutation-error toast would be redundant noise.
          { context: { skipErrorToast: true } }
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
      } else {
        toast.error(
          err instanceof Error && err.message
            ? err.message
            : `Couldn't create the invite`
        )
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
            onChange={(e) => setEmail(e.target.value)}
            placeholder="teammate@example.com"
            aria-label="Invite email address"
            autoFocus={autoFocus === `email`}
            onKeyDown={(e) => {
              if (e.key === `Enter`) void handleSend()
            }}
          />
          <Button
            className={layout === `stack` ? `self-end` : `shrink-0`}
            onClick={() => void handleSend()}
            disabled={sending || !email.trim()}
          >
            {sending ? (
              <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <UiMailIcon className="mr-2 h-4 w-4" />
            )}
            {submitLabel}
          </Button>
        </div>
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
