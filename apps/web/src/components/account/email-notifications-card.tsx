import { useState } from "react"
import { Mail } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { authClient } from "@/lib/auth/client"
import type { NotificationType } from "@/lib/domain"
import type { DigestCadence } from "@/lib/notification-email-policy"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

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
  ]

// The whole email-digest preferences card, split out of the old
// /account/notifications page (EXP-238). `verifyCallbackPath` is where the
// verification email's link lands — callers pass their own settings URL.
export function EmailNotificationsCard({
  emailPrefs,
  verifyCallbackPath,
}: {
  emailPrefs: EmailPrefs
  verifyCallbackPath: string
}) {
  const [emailEnabled, setEmailEnabled] = useState(emailPrefs.emailEnabled)
  const [typePrefs, setTypePrefs] = useState<
    Partial<Record<NotificationType, boolean>>
  >(emailPrefs.typePrefs ?? {})
  const [digest, setDigest] = useState(emailPrefs.digest)
  const [digestHour, setDigestHour] = useState(emailPrefs.digestHour)
  // REV2-52: the digest sweep never mails an unverified address. Signup fires
  // exactly ONE verification email and nothing else stamps emailVerified, so
  // a user who missed it configures a channel that silently does nothing —
  // unless the panel says so and offers a resend.
  const [verifyState, setVerifyState] = useState<
    `idle` | `sending` | `sent` | `error`
  >(`idle`)

  const transportConfigured = emailPrefs.transportConfigured
  const emailVerified = emailPrefs.emailVerified

  const resendVerification = async () => {
    if (verifyState === `sending`) return
    setVerifyState(`sending`)
    try {
      const { error } = await authClient.sendVerificationEmail({
        email: emailPrefs.email,
        callbackURL: verifyCallbackPath,
      })
      setVerifyState(error ? `error` : `sent`)
    } catch {
      setVerifyState(`error`)
    }
  }

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

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-md border bg-muted">
            <Mail className="h-5 w-5" />
          </div>
          <div className="flex-1">
            <CardTitle>Email notifications</CardTitle>
            <CardDescription>
              Notifications still unread are bundled into one digest email.
            </CardDescription>
          </div>
          <Switch
            checked={emailEnabled}
            onCheckedChange={handleEmailEnabled}
            disabled={!transportConfigured}
            aria-label="Email notifications"
          />
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
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

        {transportConfigured && !emailVerified && (
          <div className="flex items-start gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            <div className="flex-1 text-muted-foreground">
              <p className="font-medium text-foreground">
                Verify your email to receive digest emails
              </p>
              <p className="mt-0.5 text-xs">
                Digests are never sent to an unverified address. In-app and
                push notifications are unaffected.
                {verifyState === `sent` &&
                  ` Verification email sent to ${emailPrefs.email}.`}
                {verifyState === `error` &&
                  ` Couldn't send the verification email. Try again in a moment.`}
              </p>
            </div>
            <Button
              size="sm"
              variant="outline"
              className="shrink-0"
              disabled={verifyState === `sending` || verifyState === `sent`}
              onClick={() => void resendVerification()}
            >
              {verifyState === `sending`
                ? `Sending…`
                : verifyState === `sent`
                  ? `Sent`
                  : `Resend`}
            </Button>
          </div>
        )}

        <div className="space-y-3">
          {TYPE_ROWS.map((row) => (
            <div key={row.type} className="flex items-center gap-3">
              <div className="flex-1">
                <Label htmlFor={`type-${row.type}`}>{row.label}</Label>
                <p className="text-xs text-muted-foreground">{row.hint}</p>
              </div>
              <Switch
                id={`type-${row.type}`}
                checked={typePrefs[row.type] !== false}
                onCheckedChange={(next) => handleTypeToggle(row.type, next)}
                disabled={!transportConfigured}
              />
            </div>
          ))}
        </div>

        <Separator />

        <div className="flex items-center gap-3">
          <div className="flex-1">
            <Label>Delivery</Label>
            <p className="text-xs text-muted-foreground">
              How often the digest goes out.
            </p>
          </div>
          <Select
            value={digest}
            onValueChange={(next) => handleDigest(next as DigestCadence)}
            disabled={!transportConfigured || !emailEnabled}
          >
            <SelectTrigger className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="off">Hourly digest</SelectItem>
              <SelectItem value="daily">Daily digest</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {digest === `daily` && (
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <Label>Send time</Label>
              <p className="text-xs text-muted-foreground">
                Full hours only, in your timezone.
              </p>
            </div>
            <Select
              value={String(digestHour)}
              onValueChange={(next) => handleDigestHour(Number(next))}
              disabled={!transportConfigured || !emailEnabled}
            >
              <SelectTrigger className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {HOUR_OPTIONS.map((hour) => (
                  <SelectItem key={hour} value={String(hour)}>
                    {`${hour}:00`}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
