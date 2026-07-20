import { useCallback, useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  Check,
  Code2,
  Copy,
  LifeBuoy,
  Loader2,
  MessageSquarePlus,
  Pencil,
  Trash2,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { buildWidgetSnippet } from "@/lib/widget-snippet"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { useTeamBoards } from "@/hooks/use-team-data"
import {
  DEFAULT_ACCENT,
  WidgetLauncherPreview,
} from "@/components/widget-launcher-preview"
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
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import type { Team } from "@/db/schema"

type WidgetList = Awaited<ReturnType<typeof trpc.widgets.list.query>>

function buildSnippet(publicKey: string): string {
  return buildWidgetSnippet(publicKey, window.location.origin)
}

function parseDomains(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((domain) => domain.trim())
    .filter((domain) => domain.length > 0)
}

type WidgetPosition = `bottom-left` | `bottom-right`
// The settings-facing shape of formConfig.modes: a single pick instead of a
// multi-select (there are only three valid combinations).
type WidgetModeChoice = `feedback` | `support` | `both`

// The styling knobs stored in widget_configs.form_config (jsonb) — read
// defensively, rows may predate any of the fields.
function readFormConfig(raw: Record<string, unknown> | null): {
  buttonLabel: string
  accentColor: string
  position: WidgetPosition
  emailRequired: boolean
  mode: WidgetModeChoice
} {
  const modes = Array.isArray(raw?.modes) ? raw.modes : []
  const hasSupport = modes.includes(`support`)
  const hasFeedback = modes.includes(`feedback`) || !hasSupport
  return {
    buttonLabel: typeof raw?.buttonLabel === `string` ? raw.buttonLabel : ``,
    accentColor:
      typeof raw?.accentColor === `string` &&
      /^#[0-9a-fA-F]{6}$/.test(raw.accentColor)
        ? raw.accentColor
        : ``,
    position: raw?.position === `bottom-right` ? `bottom-right` : `bottom-left`,
    emailRequired: raw?.emailRequired === true,
    mode: hasSupport ? (hasFeedback ? `both` : `support`) : `feedback`,
  }
}

function modesForChoice(
  choice: WidgetModeChoice
): Array<`feedback` | `support`> {
  if (choice === `both`) return [`feedback`, `support`]
  return [choice]
}

export function TeamWidgetSection({
  team,
}: {
  team: Team
}) {
  const teamId = team.id
  const boards = useTeamBoards(teamId).filter(
    (board) => !board.archivedAt
  )
  const [widgets, setWidgets] = useState<WidgetList>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [copiedId, setCopiedId] = useState<string | null>(null)
  const [snippetTarget, setSnippetTarget] = useState<WidgetList[number] | null>(
    null
  )

  // Create/edit dialog state (editTarget === null → create).
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<WidgetList[number] | null>(null)
  const [formName, setFormName] = useState(``)
  const [formBoardId, setFormBoardId] = useState<string>(``)
  const [formDomains, setFormDomains] = useState(``)
  const [formButtonLabel, setFormButtonLabel] = useState(``)
  // Empty string = "use the widget default" (the accentColor key is omitted).
  const [formAccent, setFormAccent] = useState(``)
  const [formPosition, setFormPosition] =
    useState<WidgetPosition>(`bottom-left`)
  const [formEmailRequired, setFormEmailRequired] = useState(false)
  const [formMode, setFormMode] = useState<WidgetModeChoice>(`feedback`)
  const [saving, setSaving] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)

  // Team-level helpdesk switch (EXP-180 — replaced the per-board flag).
  const [helpdeskBusy, setHelpdeskBusy] = useState(false)
  const [helpdeskError, setHelpdeskError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setWidgets(await trpc.widgets.list.query({ teamId }))
      setError(null)
    } catch {
      setError(`Couldn't load widgets — are you an owner of this team?`)
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
    setFormName(``)
    setFormBoardId(boards[0]?.id ?? ``)
    setFormDomains(``)
    setFormButtonLabel(``)
    setFormAccent(``)
    setFormPosition(`bottom-left`)
    setFormEmailRequired(false)
    setFormMode(`feedback`)
    setFormError(null)
    setDialogOpen(true)
  }

  const openEdit = (widget: WidgetList[number]) => {
    const config = readFormConfig(widget.formConfig)
    setEditTarget(widget)
    setFormName(widget.name)
    setFormBoardId(widget.boardId ?? ``)
    setFormDomains(widget.allowedDomains.join(`\n`))
    setFormButtonLabel(config.buttonLabel)
    setFormAccent(config.accentColor)
    setFormPosition(config.position)
    setFormEmailRequired(config.emailRequired)
    setFormMode(config.mode)
    setFormError(null)
    setDialogOpen(true)
  }

  const buildFormConfig = () => ({
    ...(formButtonLabel.trim() ? { buttonLabel: formButtonLabel.trim() } : {}),
    ...(formAccent ? { accentColor: formAccent } : {}),
    position: formPosition,
    emailRequired: formEmailRequired,
    modes: modesForChoice(formMode),
  })

  // A feedback board is required whenever the widget offers feedback mode; a
  // support-only widget has none (tickets go to the team support inbox).
  const needsBoard = formMode !== `support`
  const canSave =
    Boolean(formName.trim()) &&
    (!needsBoard || Boolean(formBoardId)) &&
    parseDomains(formDomains).length > 0

  const save = async () => {
    if (!canSave) {
      setFormError(
        needsBoard
          ? `Name, feedback board, and at least one allowed domain are required.`
          : `Name and at least one allowed domain are required.`
      )
      return
    }
    setSaving(true)
    setFormError(null)
    const boardId = needsBoard ? formBoardId : null
    try {
      if (editTarget) {
        await trpc.widgets.update.mutate({
          widgetConfigId: editTarget.id,
          name: formName.trim(),
          boardId,
          allowedDomains: parseDomains(formDomains),
          formConfig: buildFormConfig(),
        })
      } else {
        const created = await trpc.widgets.create.mutate({
          teamId,
          boardId,
          name: formName.trim(),
          allowedDomains: parseDomains(formDomains),
          formConfig: buildFormConfig(),
        })
        setSnippetTarget({
          ...created,
          boardName:
            boards.find((board) => board.id === created.boardId)
              ?.name ?? null,
          submissionCount: 0,
        })
      }
      setDialogOpen(false)
      await refresh()
    } catch (saveError) {
      setFormError(
        saveError instanceof Error ? saveError.message : `Couldn't save widget`
      )
    } finally {
      setSaving(false)
    }
  }

  const toggleEnabled = async (widget: WidgetList[number], next: boolean) => {
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

  const deleteWidget = async (widget: WidgetList[number]) => {
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

  const copySnippet = async (widget: WidgetList[number]) => {
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
      setHelpdeskError(
        isPlanLimitError(err)
          ? `The helpdesk is available on Pro and Business plans.`
          : `Could not update the helpdesk setting.`
      )
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
            screenshot, describe the problem, and it lands here as an issue —
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
              <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Loading widgets
              </div>
            ) : error ? (
              <div className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            ) : widgets.length === 0 ? (
              <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
                No widgets yet. Create one to get an embed snippet.
              </div>
            ) : (
              widgets.map((widget) => (
                <div
                  key={widget.id}
                  className="flex flex-col gap-3 overflow-hidden rounded-md border px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
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
                          no allowed domains — widget blocked
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
                      <Code2 className="h-4 w-4" />
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
        </CardContent>

        <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
          <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>
                {editTarget ? `Edit widget` : `New widget`}
              </DialogTitle>
              <DialogDescription>
                Feedback submissions create issues on the selected board;
                support tickets land in the team&apos;s Support inbox. The key
                in the snippet is public; restrict it to your domains.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="widget-name">Name</Label>
                <Input
                  id="widget-name"
                  placeholder="Acme App"
                  value={formName}
                  onChange={(event) => setFormName(event.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label>Modes</Label>
                <Select
                  value={formMode}
                  onValueChange={(value) =>
                    setFormMode(value as WidgetModeChoice)
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="feedback">Feedback</SelectItem>
                    <SelectItem value="support">Support</SelectItem>
                    <SelectItem value="both">Feedback + support</SelectItem>
                  </SelectContent>
                </Select>
                {formMode !== `feedback` && (
                  <p className="text-xs text-muted-foreground">
                    Support files helpdesk tickets — visitors get a
                    reply-by-email conversation. Requires the helpdesk to be
                    enabled for this team (below).
                  </p>
                )}
              </div>
              {needsBoard && (
                <div className="space-y-2">
                  <Label>Feedback board</Label>
                  <Select
                    value={formBoardId}
                    onValueChange={setFormBoardId}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select a board" />
                    </SelectTrigger>
                    <SelectContent>
                      {boards.map((board) => (
                        <SelectItem key={board.id} value={board.id}>
                          {board.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground">
                    Feedback submissions land on this board as issues.
                  </p>
                </div>
              )}
              <div className="space-y-2">
                <Label htmlFor="widget-domains">Allowed domains</Label>
                <Textarea
                  id="widget-domains"
                  placeholder={`app.example.com\n*.example.com\nlocalhost:5173`}
                  value={formDomains}
                  onChange={(event) => setFormDomains(event.target.value)}
                  className="min-h-20 font-mono text-xs"
                />
                <p className="text-xs text-muted-foreground">
                  One per line. `*.example.com` matches subdomains only. At
                  least one domain is required.
                </p>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="widget-button-label">Button label</Label>
                  <Input
                    id="widget-button-label"
                    placeholder="Feedback"
                    maxLength={40}
                    value={formButtonLabel}
                    onChange={(event) => setFormButtonLabel(event.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="widget-accent">Accent color</Label>
                  <div className="flex items-center gap-2">
                    <Input
                      id="widget-accent"
                      type="color"
                      value={formAccent || DEFAULT_ACCENT}
                      onChange={(event) => setFormAccent(event.target.value)}
                      className="h-9 w-14 cursor-pointer p-1"
                    />
                    {formAccent ? (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setFormAccent(``)}
                      >
                        Reset
                      </Button>
                    ) : (
                      <span className="text-xs text-muted-foreground">
                        Default
                      </span>
                    )}
                  </div>
                </div>
                <div className="space-y-2">
                  <Label>Position</Label>
                  <Select
                    value={formPosition}
                    onValueChange={(value) =>
                      setFormPosition(value as WidgetPosition)
                    }
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="bottom-left">Bottom left</SelectItem>
                      <SelectItem value="bottom-right">Bottom right</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="widget-email-required">Require email</Label>
                  <div className="flex h-9 items-center">
                    <Switch
                      id="widget-email-required"
                      checked={formEmailRequired}
                      onCheckedChange={setFormEmailRequired}
                    />
                  </div>
                </div>
              </div>
              <div className="flex items-center justify-between rounded-md border bg-muted/30 px-3 py-3">
                <span className="text-xs text-muted-foreground">
                  Launcher preview
                </span>
                <WidgetLauncherPreview
                  accentColor={formAccent || undefined}
                  label={formButtonLabel.trim() || undefined}
                />
              </div>
              {formError && (
                <p className="text-sm text-destructive">{formError}</p>
              )}
            </div>
            <DialogFooter>
              <Button
                variant="ghost"
                onClick={() => setDialogOpen(false)}
                disabled={saving}
              >
                Cancel
              </Button>
              <Button onClick={save} disabled={saving || !canSave}>
                {saving ? `Saving…` : editTarget ? `Save` : `Create widget`}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        <Dialog
          open={snippetTarget !== null}
          onOpenChange={(open) => !open && setSnippetTarget(null)}
        >
          <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-xl">
            <DialogHeader>
              <DialogTitle>Embed snippet</DialogTitle>
              <DialogDescription>
                Paste this before the closing {`</body>`} tag of your site.
              </DialogDescription>
            </DialogHeader>
            {snippetTarget && (
              <>
                <pre className="max-h-72 overflow-auto rounded-md border bg-muted/30 p-3 text-xs">
                  {buildSnippet(snippetTarget.publicKey)}
                </pre>
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
