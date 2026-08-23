import { useCallback, useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import {
  Check,
  CodeXml,
  Copy,
  LifeBuoy,
  LoaderCircle,
  MessageSquarePlus,
  Pencil,
  Sparkles,
  Trash2,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { buildWidgetSnippet } from "@/lib/widget-snippet"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { useBillingPlan } from "@/hooks/use-billing"
import { useTeamBoards } from "@/hooks/use-team-data"
import { UsageBar } from "@/components/team/billing-section"
import {
  WidgetConfigDialog,
  type WidgetListItem,
} from "@/components/team/widget-config-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Switch } from "@/components/ui/switch"
import type { Team } from "@/db/schema"

function buildSnippet(publicKey: string): string {
  return buildWidgetSnippet(publicKey, window.location.origin)
}

export function TeamWidgetSection({ team }: { team: Team }) {
  const teamId = team.id
  const boards = useTeamBoards(teamId)
  // Free-plan-only submissions meter (EXP-459). Self-hosted resolves to
  // `unlimited` without a network call, so the block never renders there.
  const billingPlan = useBillingPlan(teamId)
  const [widgets, setWidgets] = useState<WidgetListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [snippetTarget, setSnippetTarget] = useState<WidgetListItem | null>(
    null
  )

  // Create/edit dialog state (editTarget === null → create). The dialog
  // itself lives in widget-config-dialog.tsx (EXP-435).
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<WidgetListItem | null>(null)

  // Team-level helpdesk switch (EXP-180 — replaced the per-board flag).
  const [helpdeskBusy, setHelpdeskBusy] = useState(false)
  const [helpdeskError, setHelpdeskError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setWidgets(await trpc.widgets.list.query({ teamId }))
      setError(null)
    } catch {
      setError(`Couldn't load widgets. Are you an owner of this team?`)
    } finally {
      setLoading(false)
    }
  }, [teamId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  const openCreate = () => {
    setEditTarget(null)
    setDialogOpen(true)
  }

  const openEdit = (widget: WidgetListItem) => {
    setEditTarget(widget)
    setDialogOpen(true)
  }

  const toggleEnabled = async (widget: WidgetListItem, next: boolean) => {
    setBusyId(widget.id)
    try {
      await trpc.widgets.update.mutate({
        widgetConfigId: widget.id,
        enabled: next,
      })
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  const deleteWidget = async (widget: WidgetListItem) => {
    if (
      !window.confirm(
        `Delete the "${widget.name}" widget? Sites using its key stop working immediately. Issues it created are kept.`
      )
    ) {
      return
    }
    setBusyId(widget.id)
    try {
      await trpc.widgets.delete.mutate({ widgetConfigId: widget.id })
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  const copySnippet = async (widget: WidgetListItem) => {
    await navigator.clipboard.writeText(buildSnippet(widget.publicKey))
    setCopiedId(widget.id)
    window.setTimeout(() => setCopiedId(null), 1_500)
  }

  const toggleHelpdesk = async (enabled: boolean) => {
    setHelpdeskBusy(true)
    setHelpdeskError(null)
    try {
      await trpc.teams.update.mutate({
        id: teamId,
        helpdeskEnabled: enabled,
      })
    } catch (err) {
      if (isPlanLimitError(err)) {
        setHelpdeskError(`The helpdesk is available on the Team plan.`)
      } else if (
        err instanceof TRPCClientError &&
        err.data?.code === `PRECONDITION_FAILED`
      ) {
        // A non-plan-limit precondition failure is an actionable SETUP error —
        // REV2-10(c)'s transport gate refuses `helpdeskEnabled: true` with a
        // message naming AWS_SES_REGION / SMTP_HOST. This toggle is the only
        // place that gate is ever hit, so the server's own wording must reach
        // the owner instead of the generic fallback.
        setHelpdeskError(err.message)
      } else {
        setHelpdeskError(`Could not update the helpdesk setting.`)
      }
    } finally {
      setHelpdeskBusy(false)
    }
  }

  return (
    <div className="space-y-6">
      {/* Anchor target for the "Getting started" widget card's settings link. */}
      <Card id="feedback-widget" className="scroll-mt-6">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <MessageSquarePlus className="h-4 w-4" />
            Exponential widget
          </CardTitle>
          <CardDescription>
            Embed the Exponential widget on your own site: visitors capture a
            screenshot, describe the problem, and it lands here as an issue,
            with reporter email and page context attached.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex justify-end">
            <Button size="sm" onClick={openCreate}>
              New widget
            </Button>
          </div>

          <div className="space-y-2">
            {loading ? (
              <div className="flex items-center gap-2 rounded-md border border-glass-stroke bg-glass-row px-3 py-2 text-sm text-muted-foreground">
                <LoaderCircle className="h-4 w-4 animate-spin" />
                Loading widgets
              </div>
            ) : error ? (
              <div className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            ) : widgets.length === 0 ? (
              <div className="rounded-md border border-glass-stroke bg-glass-row px-3 py-2 text-sm text-muted-foreground">
                No widgets yet. Create one to get an embed snippet.
              </div>
            ) : (
              widgets.map((widget) => (
                <div
                  key={widget.id}
                  className="flex flex-col gap-3 overflow-hidden rounded-md border border-glass-stroke bg-glass-row px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="break-all text-sm font-medium">
                        {widget.name}
                      </span>
                      {widget.boardName && (
                        <Badge variant="secondary">{widget.boardName}</Badge>
                      )}
                      {!widget.enabled && (
                        <Badge variant="outline">disabled</Badge>
                      )}
                    </div>
                    <div className="mt-1 flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                      <span>
                        {widget.submissionCount}
                        {` `}
                        {widget.submissionCount === 1
                          ? `submission`
                          : `submissions`}
                        {` · `}
                      </span>
                      {widget.allowedDomains.length === 0 ? (
                        // Legacy pre-EXP-209 config: empty allowlist is now
                        // denied at serve time, so the widget is dead until
                        // domains are added.
                        <span className="text-amber-500">
                          Widget blocked: no allowed domains
                        </span>
                      ) : (
                        widget.allowedDomains.map((domain) => (
                          <Badge
                            key={domain}
                            variant="outline"
                            className="px-1.5 py-0 text-[11px] font-normal"
                          >
                            {domain}
                          </Badge>
                        ))
                      )}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Switch
                      checked={widget.enabled}
                      disabled={busyId === widget.id}
                      onCheckedChange={(next) => toggleEnabled(widget, next)}
                      aria-label={`Enable ${widget.name}`}
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setSnippetTarget(widget)}
                      aria-label={`Show snippet for ${widget.name}`}
                    >
                      <CodeXml className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => openEdit(widget)}
                      aria-label={`Edit ${widget.name}`}
                    >
                      <Pencil className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => deleteWidget(widget)}
                      disabled={busyId === widget.id}
                      aria-label={`Delete ${widget.name}`}
                    >
                      <Trash2 className="h-4 w-4 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))
            )}
          </div>

          {/* Free plan caps widget submissions at 60/hour per team (EXP-459);
              paid tiers are unlimited, so this meter — the ONLY place the
              limit is promoted — renders on free only. */}
          {billingPlan?.plan === `free` && (
            <div className="space-y-2 border-t pt-4">
              <UsageBar
                label="Widget submissions (last hour)"
                current={billingPlan.usage.widgetSubmissionsLastHour}
                max={billingPlan.limits.widgetSubmissionsPerHour}
              />
              <div className="flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
                <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" />
                <span className="min-w-0 flex-1">
                  Upgrade to Team for unlimited submissions.
                </span>
                <Button asChild size="sm" variant="outline">
                  <Link
                    to="/t/$teamSlug/settings/billing"
                    params={{ teamSlug: team.slug }}
                    hash="plans"
                  >
                    Upgrade
                  </Link>
                </Button>
              </div>
            </div>
          )}
        </CardContent>

        <WidgetConfigDialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          teamId={teamId}
          boards={boards}
          editTarget={editTarget}
          onSaved={async (created) => {
            if (created) setSnippetTarget(created)
            await refresh()
          }}
        />

        <Dialog
          open={snippetTarget !== null}
          onOpenChange={(open) => !open && setSnippetTarget(null)}
        >
          <DialogContent className="sm:max-w-xl">
            <DialogHeader>
              <DialogTitle>Embed snippet</DialogTitle>
              <DialogDescription>
                Paste this before the closing {`</body>`} tag of your site.
              </DialogDescription>
            </DialogHeader>
            {snippetTarget && (
              <>
                <DialogBody>
                  <pre className="overflow-x-auto rounded-md border bg-muted/30 p-3 text-xs">
                    {buildSnippet(snippetTarget.publicKey)}
                  </pre>
                </DialogBody>
                <DialogFooter>
                  <Button onClick={() => copySnippet(snippetTarget)}>
                    {copiedId === snippetTarget.id ? (
                      <>
                        <Check className="mr-1 h-4 w-4" /> Copied
                      </>
                    ) : (
                      <>
                        <Copy className="mr-1 h-4 w-4" /> Copy snippet
                      </>
                    )}
                  </Button>
                </DialogFooter>
              </>
            )}
          </DialogContent>
        </Dialog>
      </Card>

      {/* Team-level helpdesk switch (owner-only page). Lives with the
          widget settings because support tickets arrive through the widget. */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <LifeBuoy className="h-4 w-4" />
            Helpdesk
          </CardTitle>
          <CardDescription>
            Give this team a shared support inbox. Support tickets from the
            widget land there.
          </CardDescription>
          <CardAction>
            <Switch
              checked={team.helpdeskEnabled}
              disabled={helpdeskBusy}
              onCheckedChange={(next) => void toggleHelpdesk(next)}
              aria-label="Enable the helpdesk"
            />
          </CardAction>
        </CardHeader>
        {(helpdeskError || team.helpdeskEnabled) && (
          <CardContent className="space-y-2">
            {helpdeskError && (
              <p className="text-xs text-destructive">{helpdeskError}</p>
            )}
            {team.helpdeskEnabled && (
              <Button variant="outline" size="sm" asChild className="w-fit">
                <Link
                  to="/t/$teamSlug/support"
                  params={{ teamSlug: team.slug }}
                >
                  Open Support inbox
                </Link>
              </Button>
            )}
          </CardContent>
        )}
      </Card>
    </div>
  )
}
