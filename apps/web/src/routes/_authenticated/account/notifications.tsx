import { useState } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { ArrowLeft, Mail } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import type { NotificationType } from "@/lib/domain"
import type { DigestCadence } from "@/lib/notification-email-policy"
import { Button } from "@/components/ui/button"
import { DeleteAccountSection } from "@/components/account/delete-account-section"
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

export const Route = createFileRoute(`/_authenticated/account/notifications`)({
  loader: async () => {
    const emailPrefs = await trpc.notifications.emailPrefs.query()
    return { emailPrefs }
  },
  component: AccountNotifications,
})

const TYPE_ROWS: Array<{ type: NotificationType; label: string; hint: string }> =
  [
    {
      type: `issue_created`,
      label: `New feedback`,
      hint: `A new issue is filed in your workspace via the feedback widget.`,
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
  ]

function AccountNotifications() {
  const { emailPrefs } = Route.useLoaderData()
  const [emailEnabled, setEmailEnabled] = useState(emailPrefs.emailEnabled)
  const [typePrefs, setTypePrefs] = useState<
    Partial<Record<NotificationType, boolean>>
  >(emailPrefs.typePrefs ?? {})
  const [digest, setDigest] = useState(emailPrefs.digest)

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

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <Button asChild variant="ghost" size="sm" className="-ml-2 mb-2">
          <Link to="/">
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </Button>
        <h1 className="text-2xl font-bold">Account</h1>
        <p className="text-sm text-muted-foreground">
          Email notification preferences and account management. In-app and
          push notifications are always on.
        </p>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-md border bg-muted">
              <Mail className="h-5 w-5" />
            </div>
            <div className="flex-1">
              <CardTitle>Email notifications</CardTitle>
              <CardDescription>
                Email is the catch-up channel: notifications still unread an
                hour after the push are bundled into one digest email, with
                deep links straight to each issue.
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
                  disabled={!transportConfigured || !emailEnabled}
                />
              </div>
            ))}
          </div>

          <Separator />

          <div className="flex items-center gap-3">
            <div className="flex-1">
              <Label>Delivery</Label>
              <p className="text-xs text-muted-foreground">
                How often unread notifications are bundled into one email.
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
        </CardContent>
      </Card>

      <DeleteAccountSection />
    </div>
  )
}
