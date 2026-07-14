import { useCallback, useEffect, useState } from "react"
import {
  Check,
  Code2,
  Copy,
  Loader2,
  MessageSquarePlus,
  Pencil,
  Trash2,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { buildWidgetSnippet } from "@/lib/widget-snippet"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import { Badge } from "@/components/ui/badge"
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

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

export function WorkspaceWidgetSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const projects = useWorkspaceProjects(workspaceId).filter(
    (project) => !project.archivedAt
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
  const [formProjectId, setFormProjectId] = useState<string>(``)
  const [formDomains, setFormDomains] = useState(``)
  const [saving, setSaving] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setWidgets(await trpc.widgets.list.query({ workspaceId }))
      setError(null)
    } catch {
      setError(`Couldn't load widgets — are you an owner of this workspace?`)
    } finally {
      setLoading(false)
    }
  }, [workspaceId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  const openCreate = () => {
    setEditTarget(null)
    setFormName(``)
    setFormProjectId(projects[0]?.id ?? ``)
    setFormDomains(``)
    setFormError(null)
    setDialogOpen(true)
  }

  const openEdit = (widget: WidgetList[number]) => {
    setEditTarget(widget)
    setFormName(widget.name)
    setFormProjectId(widget.projectId)
    setFormDomains(widget.allowedDomains.join(`\n`))
    setFormError(null)
    setDialogOpen(true)
  }

  const save = async () => {
    if (!formName.trim() || !formProjectId) {
      setFormError(`Name and project are required.`)
      return
    }
    setSaving(true)
    setFormError(null)
    try {
      if (editTarget) {
        await trpc.widgets.update.mutate({
          widgetConfigId: editTarget.id,
          name: formName.trim(),
          projectId: formProjectId,
          allowedDomains: parseDomains(formDomains),
        })
      } else {
        const created = await trpc.widgets.create.mutate({
          workspaceId,
          projectId: formProjectId,
          name: formName.trim(),
          allowedDomains: parseDomains(formDomains),
        })
        setSnippetTarget({
          ...created,
          projectName:
            projects.find((project) => project.id === created.projectId)
              ?.name ?? ``,
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

  return (
    // Anchor target for the "Getting started" widget card's settings link.
    <Card id="feedback-widget" className="scroll-mt-6">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <MessageSquarePlus className="h-4 w-4" />
          Feedback widget
        </CardTitle>
        <CardDescription>
          Embed a feedback widget on your own site: visitors capture a
          screenshot, describe the problem, and it lands here as an issue —
          with reporter email and page context attached.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex justify-end">
          <Button size="sm" onClick={openCreate} disabled={projects.length === 0}>
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
                    <Badge variant="secondary">{widget.projectName}</Badge>
                    {!widget.enabled && (
                      <Badge variant="outline">disabled</Badge>
                    )}
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>
                      {widget.submissionCount}{` `}
                      {widget.submissionCount === 1
                        ? `submission`
                        : `submissions`}
                      {` · `}
                    </span>
                    {widget.allowedDomains.length === 0 ? (
                      <span className="text-amber-500">
                        any website can use this key
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
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editTarget ? `Edit widget` : `New feedback widget`}
            </DialogTitle>
            <DialogDescription>
              Submissions create issues in the selected project. The key in
              the snippet is public; restrict it to your domains.
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
              <Label>Project</Label>
              <Select value={formProjectId} onValueChange={setFormProjectId}>
                <SelectTrigger>
                  <SelectValue placeholder="Select a project" />
                </SelectTrigger>
                <SelectContent>
                  {projects.map((project) => (
                    <SelectItem key={project.id} value={project.id}>
                      {project.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
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
                One per line. `*.example.com` matches subdomains only. Leave
                empty to allow any website (not recommended).
              </p>
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
            <Button onClick={save} disabled={saving}>
              {saving ? `Saving…` : editTarget ? `Save` : `Create widget`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
  )
}
