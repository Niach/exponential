import { useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { Combobox, GlassGroup, GlassToggleRow } from "@exp/ui"
import type { NotificationType } from "@/lib/domain"
import type { DigestCadence } from "@/lib/notification-email-policy"

export type EmailPrefs = Awaited<
  ReturnType<typeof trpc.notifications.emailPrefs.query>
>

const HOUR_OPTIONS = Array.from({ length: 24 }, (_, hour) => hour)

const TYPE_ROWS: Array<{ type: NotificationType; label: string; hint: string }> =
  [
    {
      type: `issue_created`,
      label: `New feedback`,
      hint: `A new issue is filed in your team via the feedback widget.`,
    },
    {
      type: `issue_assigned`,
      label: `Assigned to you`,
      hint: `Someone assigns an issue to you.`,
    },
    {
      type: `issue_comment`,
      label: `Comments`,
      hint: `New comments on issues you're subscribed to.`,
    },
    {
      type: `issue_mention`,
      label: `Mentions`,
      hint: `Someone @mentions you in a description or comment.`,
    },
    {
      type: `issue_status_changed`,
      label: `Status changes`,
      hint: `An issue you're subscribed to changes status.`,
    },
    {
      type: `pr_opened`,
      label: `Pull request opened`,
      hint: `A PR is opened for an issue you follow.`,
    },
    {
      type: `pr_merged`,
      label: `Pull request merged`,
      hint: `A PR for an issue you follow is merged.`,
    },
    // REV2-51: the digest honors this pref generically — the panel just never
    // offered it, so wanting issue mail but not helpdesk mail meant the
    // global kill switch.
    {
      type: `support_reply`,
      label: `Support tickets`,
      hint: `New helpdesk tickets and reporter replies in your teams.`,
    },
    // EXP-980: a walled run reads `running`; this is how its owner hears.
    {
      type: `session_blocked`,
      label: `Blocked runs`,
      hint: `One of your coding runs hits a rate limit.`,
    },
  ]

// The whole email-digest preferences card, split out of the old
// /account/notifications page (EXP-238).
export function EmailNotificationsCard({
  emailPrefs,
}: {
  emailPrefs: EmailPrefs
}) {
  const [emailEnabled, setEmailEnabled] = useState(emailPrefs.emailEnabled)
  const [typePrefs, setTypePrefs] = useState<
    Partial<Record<NotificationType, boolean>>
  >(emailPrefs.typePrefs ?? {})
  const [digest, setDigest] = useState(emailPrefs.digest)
  const [digestHour, setDigestHour] = useState(emailPrefs.digestHour)
  // EXP-801: servers predating the pref omit the field — treat as allowed.
  const [allowAgentMessages, setAllowAgentMessages] = useState(
    emailPrefs.allowAgentMessages !== false
  )
  const transportConfigured = emailPrefs.transportConfigured

  const handleEmailEnabled = (next: boolean) => {
    setEmailEnabled(next)
    void trpc.notifications.updateEmailPrefs
      .mutate({ emailEnabled: next })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  const handleTypeToggle = (type: NotificationType, next: boolean) => {
    const merged = { ...typePrefs, [type]: next }
    setTypePrefs(merged)
    void trpc.notifications.updateEmailPrefs
      .mutate({ typePrefs: merged })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  const handleDigest = (next: DigestCadence) => {
    setDigest(next)
    void trpc.notifications.updateEmailPrefs
      .mutate({ digest: next })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  const handleDigestHour = (next: number) => {
    setDigestHour(next)
    void trpc.notifications.updateEmailPrefs
      .mutate({ digestHour: next })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  const handleAllowAgentMessages = (next: boolean) => {
    setAllowAgentMessages(next)
    void trpc.notifications.updateEmailPrefs
      .mutate({ allowAgentMessages: next })
      .catch((err) => console.error(`[prefs] update failed:`, err))
  }

  // EXP-1054: ONE list — the master email switch, the per-type switches and
  // the agents block are rows of the same group (the IDE's
  // notifications_prefs.rs twin); only Delivery + Send time keep a shell of
  // their own. The "no mail transport" notice sits above, not inside.
  return (
    <div className="space-y-3">
      {!transportConfigured && (
        <div className="rounded-md border bg-muted p-3 text-sm text-muted-foreground">
          Email sending is not configured on this server. Set
          <code className="mx-1 rounded bg-background px-1 py-0.5 text-xs">
            AWS_SES_REGION
          </code>
          or
          <code className="mx-1 rounded bg-background px-1 py-0.5 text-xs">
            SMTP_HOST
          </code>
          to enable it.
        </div>
      )}

      <GlassGroup>
        <GlassToggleRow
          id="email-enabled"
          label="Email notifications"
          description="Notifications still unread are bundled into one digest email."
          checked={emailEnabled}
          onCheckedChange={handleEmailEnabled}
          disabled={!transportConfigured}
        />
        {TYPE_ROWS.map((row) => (
          <GlassToggleRow
            key={row.type}
            id={`type-${row.type}`}
            label={row.label}
            description={row.hint}
            checked={typePrefs[row.type] !== false}
            onCheckedChange={(next) => handleTypeToggle(row.type, next)}
            disabled={!transportConfigured}
          />
        ))}
        {/* EXP-801: a BLOCK, not a delivery mute — off means another member's
            agent cannot message this user over MCP at all (no inbox row, no
            push). The user's own agents always get through, so the row stays
            live whatever the email transport says. */}
        <GlassToggleRow
          id="allow-agent-messages"
          label="Messages from teammates' agents"
          description="Let other members' agents send you a notification over MCP. Your own agents can always reach you."
          checked={allowAgentMessages}
          onCheckedChange={handleAllowAgentMessages}
        />
      </GlassGroup>

      <GlassGroup>
        <div className="flex flex-col">
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Delivery"
            value={digest}
            onChange={(next) => {
              if (next !== null) handleDigest(next as DigestCadence)
            }}
            options={[
              { value: `off`, label: `Hourly digest` },
              { value: `daily`, label: `Daily digest` },
            ]}
            disabled={!transportConfigured || !emailEnabled}
          />
          <p className="px-4 pb-3 text-xs text-foreground/50">
            How often the digest goes out.
          </p>
        </div>

        {digest === `daily` && (
          <div className="flex flex-col">
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Send time"
              value={String(digestHour)}
              onChange={(next) => {
                if (next !== null) handleDigestHour(Number(next))
              }}
              options={HOUR_OPTIONS.map((hour) => ({
                value: String(hour),
                label: `${hour}:00`,
              }))}
              disabled={!transportConfigured || !emailEnabled}
            />
            <p className="px-4 pb-3 text-xs text-foreground/50">
              Full hours only, in your timezone.
            </p>
          </div>
        )}
      </GlassGroup>
    </div>
  )
}
