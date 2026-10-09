import { useCallback, useEffect, useState } from "react"
import { promptActions, WEB_PROMPTS } from "@/lib/prompts"
import { Link } from "@tanstack/react-router"
import { Check, Copy, LoaderCircle, Sparkles } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { buildWidgetSnippet } from "@/lib/widget-snippet"
import { useBillingPlan } from "@/hooks/use-billing"
import { useTeamBoards } from "@/hooks/use-team-data"
import { UsageBar } from "@/components/team/billing-section"
import {
  WidgetConfigDialog,
  type WidgetListItem,
} from "@/components/team/widget-config-dialog"
import {
  Pill,
  Button,
  GlassSectionHeader,
  ListRow,
  SETTINGS_LIST_CLASS,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Menu,
  Prompt,
  Switch,
  conceptIcon,
} from "@exp/ui"
import type { Team } from "@/db/schema"

const WidgetIcon = conceptIcon(`settings-widget`)
const MoreIcon = conceptIcon(`ui-more`)
const SnippetIcon = conceptIcon(`ui-copy`)
const DeleteIcon = conceptIcon(`ui-delete`)

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

  // Delete confirms first: sites using the key stop working at once.
  const [deleteTarget, setDeleteTarget] = useState<WidgetListItem | null>(null)

  const deleteWidget = async (widget: WidgetListItem) => {
    setBusyId(widget.id)
    try {
      await trpc.widgets.delete.mutate({ widgetConfigId: widget.id })
      setDeleteTarget(null)
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

  const deleteCopy = WEB_PROMPTS.deleteWidget(deleteTarget?.name ?? ``)

  return (
    <div className="space-y-6">
      {/* Anchor target for the "Getting started" widget card's settings link. */}
      <div id="feedback-widget" className="scroll-mt-6">
        <GlassSectionHeader
          leading={<WidgetIcon className="size-3.5 text-foreground/50" />}
          label="Widget"
          trailing={
            <Pill mode="action" onClick={openCreate}>
              New widget
            </Pill>
          }
        />
        <div className="space-y-4">
          <div className={SETTINGS_LIST_CLASS}>
            {loading ? (
              <ListRow className="gap-2 px-3 py-2 text-sm text-muted-foreground">
                <LoaderCircle className="h-4 w-4 animate-spin" />
                Loading widgets
              </ListRow>
            ) : error ? (
              <div className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            ) : widgets.length === 0 ? (
              <ListRow className="px-3 py-2 text-sm text-muted-foreground">
                No widgets yet. Create one to get an embed snippet.
              </ListRow>
            ) : (
              widgets.map((widget) => (
                // The row opens the editor; the enable switch and ONE ⋯ Menu
                // (snippet, delete) stay trailing at every width.
                <ListRow
                  key={widget.id}
                  interactive
                  onClick={() => openEdit(widget)}
                  aria-label={`Edit ${widget.name}`}
                  className="items-center justify-between gap-3 overflow-hidden px-3 py-2"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="break-all text-sm font-medium">
                        {widget.name}
                      </span>
                      <Pill>{widget.boardName}</Pill>
                      {!widget.enabled && <Pill>disabled</Pill>}
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
                          <Pill key={domain} className="font-normal">
                            {domain}
                          </Pill>
                        ))
                      )}
                    </div>
                  </div>
                  {/* Clicks here (and in the menu's portal) never reach
                      the row's open-the-editor handler. */}
                  <div
                    className="flex shrink-0 items-center gap-1"
                    onClick={(event) => event.stopPropagation()}
                    onKeyDown={(event) => event.stopPropagation()}
                  >
                    <Switch
                      checked={widget.enabled}
                      disabled={busyId === widget.id}
                      onCheckedChange={(next) => toggleEnabled(widget, next)}
                      aria-label={`Enable ${widget.name}`}
                    />
                    <Menu
                      align="end"
                      aria-label={`${widget.name} actions`}
                      title={widget.name}
                      entries={[
                        {
                          kind: `item`,
                          id: `snippet`,
                          label: `Embed snippet`,
                          icon: SnippetIcon,
                          onSelect: () => setSnippetTarget(widget),
                        },
                        {
                          kind: `item`,
                          id: `delete`,
                          label: `Delete`,
                          icon: DeleteIcon,
                          destructive: true,
                          disabled: busyId === widget.id,
                          onSelect: () => setDeleteTarget(widget),
                        },
                      ]}
                      trigger={
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label={`${widget.name} actions`}
                        >
                          <MoreIcon />
                        </Button>
                      }
                    />
                  </div>
                </ListRow>
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
        </div>

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

        <Prompt
          open={deleteTarget !== null}
          onOpenChange={(open) => {
            if (!open) setDeleteTarget(null)
          }}
          title={deleteCopy.title}
          body={deleteCopy.body}
          actions={promptActions(deleteCopy, {
            delete: {
              onSelect: () =>
                deleteTarget ? deleteWidget(deleteTarget) : undefined,
            },
          })}
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
      </div>
    </div>
  )
}
