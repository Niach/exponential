// Device settings (EXP-481) — the per-machine Edit view the ⋯ menu's Rename/
// Share entries collapsed into. Name + sharing are registry writes (work
// offline); agent defaults edit the SERVER-AUTHORITATIVE devices row (an
// offline machine converges on its next heartbeat), and the worktree list
// manages the machine's reported inventory through the durable command queue
// (worktree_remove / worktree_prune — queued commands run when an offline
// device returns). Owner-only: the menu only exists on "My machines" rows.
import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { contract } from "@exp/domain-contract"
import type { Device, SyncedDeviceWorktree } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { useNow } from "@/hooks/use-now"
import { deviceCollection, deviceWorktreeCollection, teamCollection } from "@/lib/collections"
import {
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import { deviceRowIsOnline, type SteerDevice } from "@/lib/steer-devices"
import { Button } from "@/components/ui/button"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  Dialog,
  DialogContent,
  DialogDescription,
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
import { Separator } from "@/components/ui/separator"
import {
  AGENT_LABELS,
  AgentOptionsFields,
  CLI_DEFAULT_EFFORT,
} from "@/components/launch-dialog/launch-options-pane"

const BranchIcon = conceptIcon(`ui-branch`)
const WarningIcon = conceptIcon(`ui-warning`)
const PruneIcon = conceptIcon(`ui-clean`)
const RemoveIcon = conceptIcon(`ui-delete`)
const OfflineIcon = conceptIcon(`ui-device-offline`)

// Radix Select forbids empty item values — "Not shared" rides a sentinel.
const NOT_SHARED = `not-shared`

interface AgentDraft {
  model: string
  effort: string
  ultracode: boolean
  planMode: boolean
  skipPermissions: boolean
}

/** One queued/in-flight command the dialog is watching. `key` anchors the
 * inline error/progress to its row (`prune` or `repo branch`). */
interface TrackedCommand {
  id: string
  key: string
}

export function DeviceSettingsDialog({
  device,
  open,
  onOpenChange,
}: {
  /** The row being edited (must be one of the caller's own machines). */
  device: SteerDevice | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const rowId = device?.rowId
  const deviceId = device?.deviceId

  // The LIVE synced row — renames/share/defaults applied elsewhere stream in;
  // the drafts below latch once per open so sync never stomps typing.
  const { data: liveRows } = useLiveQuery(
    (query) =>
      open && rowId
        ? query.from({ d: deviceCollection }).where(({ d }) => eq(d.id, rowId))
        : undefined,
    [open, rowId]
  )
  const row = (liveRows?.[0] as Device | undefined) ?? null

  const { data: worktreeRows } = useLiveQuery(
    (query) =>
      open && rowId
        ? query
            .from({ w: deviceWorktreeCollection })
            .where(({ w }) => eq(w.deviceRowId, rowId))
        : undefined,
    [open, rowId]
  )
  const worktrees = useMemo(
    () =>
      [...((worktreeRows ?? []) as SyncedDeviceWorktree[])].sort((a, b) =>
        `${a.repoFullName} ${a.branch}`.localeCompare(
          `${b.repoFullName} ${b.branch}`
        )
      ),
    [worktreeRows]
  )

  const { data: teamRows } = useLiveQuery(
    (query) => (open ? query.from({ t: teamCollection }) : undefined),
    [open]
  )
  const teams = useMemo(
    () =>
      [...(teamRows ?? [])].sort((a, b) => a.name.localeCompare(b.name)) as {
        id: string
        name: string
      }[],
    [teamRows]
  )

  const now = useNow(30_000)
  const online = row ? deviceRowIsOnline(row.lastSeenAt, now) : false
  const kind = row?.kind ?? device?.kind
  const label = row?.label ?? device?.deviceLabel ?? ``

  // ── Drafts, latched once per open (EXP-437's seededDeviceRef pattern) ────
  const seededRef = useRef<string | null>(null)
  const [nameDraft, setNameDraft] = useState(``)
  const [agentTab, setAgentTab] = useState<string>(
    contract.codingAgent.values[0]
  )
  const [defaultAgentDraft, setDefaultAgentDraft] = useState<string>(
    contract.codingAgent.values[0]
  )
  const [drafts, setDrafts] = useState<Record<string, AgentDraft>>({})
  const [defaultsDirty, setDefaultsDirty] = useState(false)

  // The defaults editor covers every agent the machine knows about — runnable
  // ∪ signed-out ∪ already-configured; an offline/quiet machine falls back to
  // the full contract set so its defaults stay editable.
  const editorAgents = useMemo(() => {
    const union = [
      ...(row?.agents ?? []),
      ...(row?.unauthedAgents ?? []),
      ...Object.keys(row?.launchDefaults?.agents ?? {}),
    ].filter((agent) => contract.codingAgent.values.includes(agent))
    const unique = [...new Set(union)]
    return unique.length > 0 ? unique : [...contract.codingAgent.values]
  }, [row?.agents, row?.unauthedAgents, row?.launchDefaults])

  useEffect(() => {
    if (!open || !row) return
    if (seededRef.current === row.id) return
    seededRef.current = row.id
    setNameDraft(row.label)
    const seeded: Record<string, AgentDraft> = {}
    for (const agent of editorAgents) {
      seeded[agent] = agentSeed(
        agent,
        row.launchDefaults?.agents?.[agent] ?? null
      )
    }
    setDrafts(seeded)
    const configuredDefault = row.launchDefaults?.defaultAgent
    const defaultAgent =
      configuredDefault && editorAgents.includes(configuredDefault)
        ? configuredDefault
        : (editorAgents[0] ?? contract.codingAgent.values[0])
    setDefaultAgentDraft(defaultAgent)
    setAgentTab(defaultAgent)
    setDefaultsDirty(false)
    setSectionErrors({})
    setTracked([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id])
  useEffect(() => {
    if (!open) seededRef.current = null
  }, [open])

  const draft = drafts[agentTab] ?? agentSeed(agentTab, null)
  const patchDraft = (patch: Partial<AgentDraft>) => {
    setDefaultsDirty(true)
    setDrafts((current) => ({
      ...current,
      [agentTab]: { ...(current[agentTab] ?? agentSeed(agentTab, null)), ...patch },
    }))
  }

  // ── Section busy/error state ─────────────────────────────────────────────
  const [busySection, setBusySection] = useState<string | null>(null)
  const [sectionErrors, setSectionErrors] = useState<Record<string, string>>(
    {}
  )
  const runSection = async (section: string, work: () => Promise<void>) => {
    if (busySection) return
    setBusySection(section)
    setSectionErrors((current) => ({ ...current, [section]: `` }))
    try {
      await work()
    } catch (error) {
      setSectionErrors((current) => ({
        ...current,
        [section]: trpcErrorMessage(error, `That didn't go through. Try again.`),
      }))
    } finally {
      setBusySection(null)
    }
  }

  const saveName = () =>
    runSection(`name`, async () => {
      if (!deviceId || !nameDraft.trim()) return
      await trpc.devices.rename.mutate({
        deviceId,
        label: nameDraft.trim(),
      })
    })

  const setShared = (teamId: string | null) =>
    runSection(`sharing`, async () => {
      if (!deviceId) return
      await trpc.devices.setShared.mutate({ deviceId, teamId })
    })

  const saveDefaults = () =>
    runSection(`defaults`, async () => {
      if (!deviceId) return
      const agents: Record<
        string,
        {
          model?: string
          effort?: string
          ultracode?: boolean
          planMode?: boolean
          skipPermissions?: boolean
        }
      > = {}
      for (const [agent, value] of Object.entries(drafts)) {
        agents[agent] = {
          model: value.model,
          effort: value.effort,
          ...(agentSupportsUltracode(agent)
            ? { ultracode: value.ultracode }
            : {}),
          ...(agentSupportsPlanMode(agent) ? { planMode: value.planMode } : {}),
          ...(agentSupportsSkipPermissions(agent)
            ? { skipPermissions: value.skipPermissions }
            : {}),
        }
      }
      await trpc.devices.setLaunchDefaults.mutate({
        deviceId,
        launchDefaults: { defaultAgent: defaultAgentDraft, agents },
      })
      setDefaultsDirty(false)
    })

  // ── Worktree commands (durable queue + poll while open) ──────────────────
  const [tracked, setTracked] = useState<TrackedCommand[]>([])
  const [removeTarget, setRemoveTarget] =
    useState<SyncedDeviceWorktree | null>(null)

  const commandKey = (worktree: SyncedDeviceWorktree) =>
    `${worktree.repoFullName} ${worktree.branch}`

  const queueCommand = async (
    key: string,
    input:
      | { kind: `worktree_prune` }
      | { kind: `worktree_remove`; repoFullName: string; branch: string }
  ) => {
    if (!deviceId) return
    setSectionErrors((current) => ({ ...current, [key]: `` }))
    try {
      const { id } = await trpc.devices.createCommand.mutate({
        deviceId,
        ...input,
      })
      setTracked((current) => [...current, { id, key }])
    } catch (error) {
      setSectionErrors((current) => ({
        ...current,
        [key]: trpcErrorMessage(error, `Couldn't queue that on the machine.`),
      }))
    }
  }

  // Poll queued commands until terminal — the material outcome (a worktree
  // vanishing) additionally streams in via sync when the device re-reports.
  // Offline devices keep their commands queued server-side; poll slowly.
  useEffect(() => {
    if (!open || tracked.length === 0) return
    let cancelled = false
    const tick = async () => {
      for (const command of tracked) {
        try {
          const result = await trpc.devices.getCommand.query({
            commandId: command.id,
          })
          if (cancelled || result.status === `pending`) continue
          setTracked((current) => current.filter((c) => c.id !== command.id))
          if (result.status === `failed`) {
            setSectionErrors((current) => ({
              ...current,
              [command.key]:
                result.result ?? `The machine reported a failure.`,
            }))
          }
        } catch {
          // Transient — keep polling.
        }
      }
    }
    const interval = setInterval(() => void tick(), online ? 2_000 : 8_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [open, tracked, online])

  const pendingKey = (key: string) =>
    tracked.some((command) => command.key === key)

  const dirtyLabel = (dirty: string): string | null =>
    dirty === `tracked`
      ? `uncommitted changes`
      : dirty === `untracked`
        ? `untracked files`
        : null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-4 sm:max-h-[85dvh] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Device settings</DialogTitle>
          <DialogDescription>
            {label}
            {kind === `server` ? ` — server` : ``}
            {online ? `` : ` (offline)`}
          </DialogDescription>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
          {/* ── Name ─────────────────────────────────────────────────── */}
          <div className="space-y-2">
            <Label htmlFor="device-settings-name">Name</Label>
            <div className="flex gap-2">
              <Input
                id="device-settings-name"
                value={nameDraft}
                maxLength={255}
                onChange={(event) => setNameDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === `Enter`) void saveName()
                }}
              />
              <Button
                variant="outline"
                disabled={
                  busySection !== null ||
                  !nameDraft.trim() ||
                  nameDraft.trim() === label
                }
                onClick={() => void saveName()}
              >
                {busySection === `name` && (
                  <LoaderCircle className="animate-spin" />
                )}
                Save
              </Button>
            </div>
            {sectionErrors.name && (
              <p className="text-xs text-destructive">{sectionErrors.name}</p>
            )}
          </div>

          {/* ── Sharing (server machines only, EXP-432) ───────────────── */}
          {kind === `server` && (
            <div className="space-y-2">
              <Label htmlFor="device-settings-shared">Shared with team</Label>
              <Select
                value={row?.sharedTeamId ?? NOT_SHARED}
                onValueChange={(value) =>
                  void setShared(value === NOT_SHARED ? null : value)
                }
                disabled={busySection !== null}
              >
                <SelectTrigger id="device-settings-shared" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NOT_SHARED}>Not shared</SelectItem>
                  {teams.map((team) => (
                    <SelectItem key={team.id} value={team.id}>
                      {team.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Teammates of the shared team can start coding sessions on this
                machine.
              </p>
              {sectionErrors.sharing && (
                <p className="text-xs text-destructive">
                  {sectionErrors.sharing}
                </p>
              )}
            </div>
          )}

          <Separator />

          {/* ── Agent defaults (server-authoritative, EXP-481) ────────── */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>Agent defaults</Label>
              {!online && (
                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                  <OfflineIcon className="size-3" />
                  Applies when the device comes online.
                </span>
              )}
            </div>
            <div className="space-y-2">
              <Label
                htmlFor="device-settings-default-agent"
                className="font-normal text-muted-foreground"
              >
                Default agent
              </Label>
              <Select
                value={defaultAgentDraft}
                onValueChange={(value) => {
                  setDefaultAgentDraft(value)
                  setDefaultsDirty(true)
                }}
              >
                <SelectTrigger
                  id="device-settings-default-agent"
                  className="w-full"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {editorAgents.map((agent) => (
                    <SelectItem key={agent} value={agent}>
                      {AGENT_LABELS[agent] ?? agent}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <AgentOptionsFields
              idPrefix="device-settings"
              agent={agentTab}
              availableAgents={editorAgents}
              onAgentChange={setAgentTab}
              model={draft.model}
              onModelChange={(value) => patchDraft({ model: value })}
              effortValue={
                draft.effort === `` ? CLI_DEFAULT_EFFORT : draft.effort
              }
              onEffortChange={(value) =>
                patchDraft({
                  effort: value === CLI_DEFAULT_EFFORT ? `` : value,
                })
              }
              ultracode={draft.ultracode}
              onUltracodeChange={(value) => patchDraft({ ultracode: value })}
              planMode={draft.planMode}
              onPlanModeChange={(value) => patchDraft({ planMode: value })}
              skipPermissions={draft.skipPermissions}
              onSkipPermissionsChange={(value) =>
                patchDraft({ skipPermissions: value })
              }
            />
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={busySection !== null || !defaultsDirty}
                onClick={() => void saveDefaults()}
              >
                {busySection === `defaults` && (
                  <LoaderCircle className="animate-spin" />
                )}
                Save defaults
              </Button>
              {sectionErrors.defaults && (
                <p className="text-xs text-destructive">
                  {sectionErrors.defaults}
                </p>
              )}
            </div>
          </div>

          <Separator />

          {/* ── Worktrees (reported inventory + durable commands) ─────── */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label>Worktrees</Label>
              <Button
                variant="outline"
                size="sm"
                className="h-6 gap-1 px-2 text-xs"
                disabled={pendingKey(`prune`) || worktrees.length === 0}
                onClick={() =>
                  void queueCommand(`prune`, { kind: `worktree_prune` })
                }
              >
                {pendingKey(`prune`) ? (
                  <LoaderCircle className="size-3 animate-spin" />
                ) : (
                  <PruneIcon className="size-3" />
                )}
                Prune merged worktrees
              </Button>
            </div>
            {!online && (worktrees.length > 0 || pendingKey(`prune`)) && (
              <p className="text-xs text-muted-foreground">
                This machine is offline — queued changes run when it comes
                online.
              </p>
            )}
            {sectionErrors.prune && (
              <p className="text-xs text-destructive">{sectionErrors.prune}</p>
            )}
            {worktrees.length === 0 ? (
              <p className="py-1 text-xs text-muted-foreground">
                No worktrees reported by this machine.
              </p>
            ) : (
              worktrees.map((worktree) => {
                const key = commandKey(worktree)
                const removing = pendingKey(key)
                const dirty = dirtyLabel(worktree.dirty)
                return (
                  <div
                    key={worktree.id}
                    className="space-y-0.5 border-b border-border/30 py-1.5 last:border-b-0"
                  >
                    <div className="flex items-center gap-2">
                      <BranchIcon className="size-3.5 shrink-0 text-muted-foreground" />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs">
                        <span className="text-muted-foreground">
                          {worktree.repoFullName}
                        </span>
                        {` `}
                        {worktree.branch}
                      </span>
                      {worktree.issueIdentifier && (
                        <span className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground">
                          {worktree.issueIdentifier}
                        </span>
                      )}
                      {dirty && (
                        <span
                          className="flex shrink-0 items-center gap-0.5 text-[10px] text-amber-500"
                          title={`This worktree has ${dirty}.`}
                        >
                          <WarningIcon className="size-3" />
                          {dirty}
                        </span>
                      )}
                      {worktree.busy && (
                        <span
                          className="shrink-0 text-[10px] text-emerald-500"
                          title="A live coding session is using this worktree."
                        >
                          in use
                        </span>
                      )}
                      <Button
                        variant="ghost"
                        className="h-5 w-5 shrink-0 p-0 text-muted-foreground"
                        title={
                          worktree.busy
                            ? `A live session is using this worktree.`
                            : `Remove this worktree on the machine`
                        }
                        disabled={worktree.busy || removing}
                        onClick={() => setRemoveTarget(worktree)}
                      >
                        {removing ? (
                          <LoaderCircle className="size-3.5 animate-spin" />
                        ) : (
                          <RemoveIcon className="size-3.5" />
                        )}
                      </Button>
                    </div>
                    {sectionErrors[key] && (
                      <p className="pl-5 text-xs text-destructive">
                        {sectionErrors[key]}
                      </p>
                    )}
                  </div>
                )
              })
            )}
          </div>
        </div>

        <AlertDialog
          open={removeTarget !== null}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) setRemoveTarget(null)
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Remove worktree</AlertDialogTitle>
              <AlertDialogDescription>
                Remove {removeTarget?.branch} ({removeTarget?.repoFullName}) on
                “{label}”? The machine refuses if the worktree has uncommitted
                changes.
                {online ? `` : ` It runs when the machine comes online.`}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  const target = removeTarget
                  setRemoveTarget(null)
                  if (target) {
                    void queueCommand(commandKey(target), {
                      kind: `worktree_remove`,
                      repoFullName: target.repoFullName,
                      branch: target.branch,
                    })
                  }
                }}
              >
                Remove
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}
