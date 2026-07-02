import { useCallback, useEffect, useState } from "react"
import { Pencil, Plus, SquareTerminal, Trash2, X } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { formatArgvLine, parseArgvLine } from "@/lib/run-configs"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
import type { Project } from "@/db/schema"

type RunConfigList = Awaited<
  ReturnType<typeof trpc.runConfigs.list.query>
>[`configs`]

type EnvRow = { key: string; value: string }

type FormState = {
  id: string | null // null = creating a new config
  name: string
  command: string
  cwd: string
  envRows: EnvRow[]
}

const EMPTY_FORM: FormState = {
  id: null,
  name: ``,
  command: ``,
  cwd: ``,
  envRows: [],
}

// Per-project terminal run commands (EXP-2): stored in the DB via the
// runConfigs tRPC router, listed by the desktop apps as play-menu entries.
// Replaces the repo-file preview-config dialog. Owner-only editing; the
// desktops gate execution behind the per-device Trust & Run prompt.
export function ProjectRunConfigsDialog({
  project,
  isOwner,
  open,
  onOpenChange,
}: {
  project: Project | null
  isOwner: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const projectId = project?.id ?? null

  const [configs, setConfigs] = useState<RunConfigList | null>(null)
  const [form, setForm] = useState<FormState | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!projectId) return
    try {
      const { configs: rows } = await trpc.runConfigs.list.query({ projectId })
      setConfigs(rows)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [projectId])

  // Reset + refetch whenever a (different) project's dialog opens.
  useEffect(() => {
    if (open) {
      setConfigs(null)
      setForm(null)
      setError(null)
      void refresh()
    }
  }, [open, refresh])

  const startEdit = (config: RunConfigList[number]) => {
    setError(null)
    setForm({
      id: config.id,
      name: config.name,
      command: formatArgvLine(config.argv),
      cwd: config.cwd ?? ``,
      envRows: Object.entries(config.env).map(([key, value]) => ({
        key,
        value,
      })),
    })
  }

  const handleSave = async () => {
    if (!form || !projectId || !isOwner) return
    const name = form.name.trim()
    const argv = parseArgvLine(form.command)
    if (!name) {
      setError(`Give the command a name`)
      return
    }
    if (argv.length === 0) {
      setError(`Enter a command to run`)
      return
    }
    const env: Record<string, string> = {}
    for (const row of form.envRows) {
      const key = row.key.trim()
      if (!key) continue
      // Pre-check the server's key grammar so failures read as a sentence
      // instead of a serialized zod issue list.
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
        setError(`Invalid environment variable name: ${key}`)
        return
      }
      env[key] = row.value
    }
    const cwd = form.cwd.trim() || null

    setBusy(true)
    setError(null)
    try {
      if (form.id) {
        await trpc.runConfigs.update.mutate(
          { id: form.id, name, argv, cwd, env },
          // Failures render inline below instead of the global toast.
          { context: { skipErrorToast: true } }
        )
      } else {
        await trpc.runConfigs.create.mutate(
          { projectId, name, argv, cwd, env },
          { context: { skipErrorToast: true } }
        )
      }
      setForm(null)
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const handleDelete = async (id: string) => {
    if (!isOwner) return
    setBusy(true)
    setError(null)
    try {
      await trpc.runConfigs.delete.mutate(
        { id },
        { context: { skipErrorToast: true } }
      )
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setForm(null)
          setBusy(false)
        }
        onOpenChange(next)
      }}
    >
      <DialogContent className="sm:max-w-[34rem]">
        <DialogHeader>
          <DialogTitle>Run commands</DialogTitle>
          <DialogDescription>
            Terminal commands for {project?.name ?? `this project`}. Desktop
            apps list them in the play menu and ask you to review and trust
            the command set on each device before the first run — and again
            whenever it changes.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          {form === null ? (
            <>
              {configs === null ? (
                <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
                  Loading commands...
                </div>
              ) : configs.length === 0 ? (
                <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
                  No run commands yet.
                </div>
              ) : (
                <div className="divide-y rounded-md border">
                  {configs.map((config) => (
                    <div
                      key={config.id}
                      className="flex items-center gap-3 px-3 py-2"
                    >
                      <SquareTerminal className="h-4 w-4 shrink-0 text-muted-foreground" />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-sm font-medium">
                            {config.name}
                          </span>
                          {config.cwd && (
                            <Badge
                              variant="outline"
                              className="shrink-0 font-mono text-xs font-normal"
                            >
                              {config.cwd}
                            </Badge>
                          )}
                          {Object.keys(config.env).length > 0 && (
                            <Badge
                              variant="secondary"
                              className="shrink-0 text-xs font-normal"
                            >
                              {Object.keys(config.env).length} env
                            </Badge>
                          )}
                        </div>
                        <code className="block truncate font-mono text-xs text-muted-foreground">
                          {formatArgvLine(config.argv)}
                        </code>
                      </div>
                      {isOwner && (
                        <>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7 shrink-0 text-muted-foreground"
                            title="Edit command"
                            disabled={busy}
                            onClick={() => startEdit(config)}
                          >
                            <Pencil className="h-3.5 w-3.5" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                            title="Delete command"
                            disabled={busy}
                            onClick={() => handleDelete(config.id)}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </Button>
                        </>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="run-config-name">Name</Label>
                <Input
                  id="run-config-name"
                  value={form.name}
                  placeholder="Web dev server"
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="run-config-command">Command</Label>
                <Input
                  id="run-config-command"
                  value={form.command}
                  placeholder="bun run dev"
                  className="font-mono"
                  onChange={(e) =>
                    setForm({ ...form, command: e.target.value })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  Runs as-is without a shell — quote arguments that contain
                  spaces.
                </p>
              </div>

              <div className="space-y-2">
                <Label htmlFor="run-config-cwd">Working directory</Label>
                <Input
                  id="run-config-cwd"
                  value={form.cwd}
                  placeholder="apps/web"
                  className="font-mono"
                  onChange={(e) => setForm({ ...form, cwd: e.target.value })}
                />
                <p className="text-xs text-muted-foreground">
                  Relative to the repository root. Leave empty for the root.
                </p>
              </div>

              <div className="space-y-2">
                <Label>Environment variables</Label>
                {form.envRows.map((row, index) => (
                  <div key={index} className="flex items-center gap-2">
                    <Input
                      value={row.key}
                      placeholder="KEY"
                      className="w-[10rem] font-mono"
                      aria-label="Variable name"
                      onChange={(e) =>
                        setForm({
                          ...form,
                          envRows: form.envRows.map((r, i) =>
                            i === index ? { ...r, key: e.target.value } : r
                          ),
                        })
                      }
                    />
                    <Input
                      value={row.value}
                      placeholder="value"
                      className="flex-1 font-mono"
                      aria-label="Variable value"
                      onChange={(e) =>
                        setForm({
                          ...form,
                          envRows: form.envRows.map((r, i) =>
                            i === index ? { ...r, value: e.target.value } : r
                          ),
                        })
                      }
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground"
                      title="Remove variable"
                      onClick={() =>
                        setForm({
                          ...form,
                          envRows: form.envRows.filter((_, i) => i !== index),
                        })
                      }
                    >
                      <X className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))}
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 gap-1 px-2 text-xs text-muted-foreground"
                  onClick={() =>
                    setForm({
                      ...form,
                      envRows: [...form.envRows, { key: ``, value: `` }],
                    })
                  }
                >
                  <Plus className="h-3 w-3" />
                  Add variable
                </Button>
                <p className="text-xs text-muted-foreground">
                  PATH, LD_PRELOAD and DYLD_* are ignored.
                </p>
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          {form === null ? (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                Close
              </Button>
              {isOwner && (
                <Button
                  disabled={busy || configs === null}
                  onClick={() => {
                    setError(null)
                    setForm(EMPTY_FORM)
                  }}
                >
                  <Plus className="mr-1.5 h-3.5 w-3.5" />
                  Add command
                </Button>
              )}
            </>
          ) : (
            <>
              <Button
                variant="outline"
                onClick={() => setForm(null)}
                disabled={busy}
              >
                Cancel
              </Button>
              <Button onClick={handleSave} disabled={!isOwner || busy}>
                {busy ? `Saving...` : form.id ? `Save` : `Add`}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
