// Device settings (EXP-481) — the per-machine Edit view the ⋯ menu's Rename/
// Share entries collapsed into. Name, the EXP-622 default-machine toggle and
// sharing are registry writes (work offline); agent defaults edit the SERVER-AUTHORITATIVE devices row (an
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
  agentSupportsUltracode,
} from "@/lib/coding-launch-prefs"
import {
  deviceCanAgentLogin,
  deviceRowIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  AgentAccountBlock,
  agentLoginKey,
} from "@/components/device-agent-account"
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
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  GlassGroup,
  GlassInputRow,
  GlassPickerRow,
  GlassSectionHeader,
  GlassToggleRow,
} from "@/components/ui/glass-rows"
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

// EXP-490 autosave cadence. Defaults debounce longer than the name: every
// setLaunchDefaults call nudges the device over the relay, so coalescing a
// burst of toggles into ONE write matters more than instant persistence.
const NAME_DEBOUNCE_MS = 800
const DEFAULTS_DEBOUNCE_MS = 1_000

interface AgentDraft {
  model: string
  effort: string
  ultracode: boolean
  planMode: boolean
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

  // ── Drafts, seeded from the live row (EXP-437's seededDeviceRef pattern) ─
  // Since EXP-490 they also RESEED whenever the server-authoritative row
  // changes underneath us — but never on top of an unsaved edit, an in-flight
  // save, or (for the name) a focused input.
  const seededRef = useRef<string | null>(null)
  const [nameDraft, setNameDraft] = useState(``)
  const [agentTab, setAgentTab] = useState<string>(
    contract.codingAgent.values[0]
  )
  const [defaultAgentDraft, setDefaultAgentDraft] = useState<string>(
    contract.codingAgent.values[0]
  )
  const [drafts, setDrafts] = useState<Record<string, AgentDraft>>({})

  // ── Autosave state (EXP-490 — no Save buttons) ───────────────────────────
  // `*Pending` = edited but not yet written; `saving*` = a write is in flight.
  const [namePending, setNamePending] = useState(false)
  const [defaultsPending, setDefaultsPending] = useState(false)
  const [savingName, setSavingName] = useState(false)
  const [savingDefaults, setSavingDefaults] = useState(false)

  // The defaults editor covers every agent the machine knows about — runnable
  // ∪ signed-out ∪ already-configured; an offline/quiet machine falls back to
  // the full contract set so its defaults stay editable.
  const editorAgents = useMemo(() => {
    const union = [
      ...(row?.agents ?? []),
      ...(row?.unauthedAgents ?? []),
      ...Object.keys(row?.launchDefaults?.agents ?? {}),
      // EXP-688: an agent the machine only reported an ACCOUNT or usage for
      // still gets a tab — that tab is now where its sign-in lives.
      ...Object.keys(row?.agentAccounts ?? {}),
      ...Object.keys(row?.agentUsage ?? {}),
    ].filter((agent) => contract.codingAgent.values.includes(agent))
    const unique = [...new Set(union)]
    return unique.length > 0 ? unique : [...contract.codingAgent.values]
  }, [
    row?.agents,
    row?.unauthedAgents,
    row?.launchDefaults,
    row?.agentAccounts,
    row?.agentUsage,
  ])

  // The value we last wrote, so our OWN write doesn't reseed the drafts back
  // to the pre-write row in the window before it syncs home.
  const sentNameRef = useRef<string | null>(null)
  const sentDefaultsStampRef = useRef(0)

  /** Applies a row's launch defaults to the drafts; returns the default agent. */
  const seedDefaultsFrom = (source: Device, agents: string[]) => {
    const seeded: Record<string, AgentDraft> = {}
    for (const agent of agents) {
      seeded[agent] = agentSeed(
        agent,
        source.launchDefaults?.agents?.[agent] ?? null
      )
    }
    setDrafts(seeded)
    const configuredDefault = source.launchDefaults?.defaultAgent
    const defaultAgent =
      configuredDefault && agents.includes(configuredDefault)
        ? configuredDefault
        : (agents[0] ?? contract.codingAgent.values[0])
    setDefaultAgentDraft(defaultAgent)
    return defaultAgent
  }

  useEffect(() => {
    if (!open || !row) return
    if (seededRef.current === row.id) return
    seededRef.current = row.id
    setNameDraft(row.label)
    setAgentTab(seedDefaultsFrom(row, editorAgents))
    setNamePending(false)
    setDefaultsPending(false)
    sentNameRef.current = null
    sentDefaultsStampRef.current = 0
    setSectionErrors({})
    setTracked([])
    setCommandResults({})
    setSwitchTarget(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id])
  useEffect(() => {
    if (!open) seededRef.current = null
  }, [open])

  // ── Reseed from the live row (EXP-490) ───────────────────────────────────
  // The devices row is server-authoritative: defaults edited on the machine
  // itself or in another client stream in and must win — unless the user has
  // an unsaved edit here, in which case theirs is the newer write.
  const defaultsVersion = useMemo(
    () =>
      `${row?.launchDefaultsUpdatedAt?.getTime() ?? 0}:${JSON.stringify(
        row?.launchDefaults ?? null
      )}`,
    [row?.launchDefaultsUpdatedAt, row?.launchDefaults]
  )
  useEffect(() => {
    if (!open || !row || seededRef.current !== row.id) return
    if (defaultsPending || savingDefaults) return
    // Our own write hasn't come back through Electric yet — keep the drafts.
    if (
      (row.launchDefaultsUpdatedAt?.getTime() ?? 0) < sentDefaultsStampRef.current
    ) {
      return
    }
    seedDefaultsFrom(row, editorAgents)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id, defaultsVersion, defaultsPending, savingDefaults])

  const nameFocusedRef = useRef(false)
  useEffect(() => {
    if (!open || !row || seededRef.current !== row.id) return
    if (namePending || savingName || nameFocusedRef.current) return
    if (sentNameRef.current !== null) {
      if (row.label !== sentNameRef.current) return
      sentNameRef.current = null
    }
    setNameDraft(row.label)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, row?.id, row?.label, namePending, savingName])

  const draft = drafts[agentTab] ?? agentSeed(agentTab, null)

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

  const setShared = (teamId: string | null) =>
    runSection(`sharing`, async () => {
      if (!deviceId) return
      await trpc.devices.setShared.mutate({ deviceId, teamId })
    })

  // EXP-622: a single toggle — written straight through, no debounce. The
  // switch renders off the LIVE row, so the server clearing the previous
  // default streams back on its own.
  const setDefault = (isDefault: boolean) =>
    runSection(`default`, async () => {
      if (!deviceId) return
      await trpc.devices.setDefault.mutate({ deviceId, isDefault })
    })

  // ── Autosave (EXP-490) ───────────────────────────────────────────────────
  // Edits schedule a debounced write; blur and dialog close flush it. Timers
  // fire outside render, so the payload is built from this mirror of the
  // newest drafts rather than a stale closure. Pending clears BEFORE the
  // request goes out and comes back only on failure — last write wins, so an
  // edit made mid-flight is never mistaken for saved.
  const latest = useRef({
    deviceId,
    label,
    nameDraft,
    drafts,
    defaultAgentDraft,
    namePending,
    defaultsPending,
  })
  latest.current = {
    deviceId,
    label,
    nameDraft,
    drafts,
    defaultAgentDraft,
    namePending,
    defaultsPending,
  }

  const nameTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const defaultsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const flushName = () => {
    if (nameTimerRef.current) {
      clearTimeout(nameTimerRef.current)
      nameTimerRef.current = null
    }
    const snapshot = latest.current
    if (!snapshot.namePending) return
    const nextLabel = snapshot.nameDraft.trim()
    latest.current.namePending = false
    setNamePending(false)
    // Blank or unchanged never writes — the reseed effect restores the row's
    // name once the field loses focus.
    if (!snapshot.deviceId || !nextLabel || nextLabel === snapshot.label) return
    sentNameRef.current = nextLabel
    setSavingName(true)
    setSectionErrors((current) => ({ ...current, name: `` }))
    void trpc.devices.rename
      .mutate({ deviceId: snapshot.deviceId, label: nextLabel })
      .catch((error) => {
        sentNameRef.current = null
        latest.current.namePending = true
        setNamePending(true)
        setSectionErrors((current) => ({
          ...current,
          name: trpcErrorMessage(error, `That didn't go through. Try again.`),
        }))
      })
      .finally(() => setSavingName(false))
  }

  const flushDefaults = () => {
    if (defaultsTimerRef.current) {
      clearTimeout(defaultsTimerRef.current)
      defaultsTimerRef.current = null
    }
    const snapshot = latest.current
    if (!snapshot.defaultsPending) return
    latest.current.defaultsPending = false
    setDefaultsPending(false)
    if (!snapshot.deviceId) return
    const agents: Record<
      string,
      {
        model?: string
        effort?: string
        ultracode?: boolean
        planMode?: boolean
      }
    > = {}
    for (const [agent, value] of Object.entries(snapshot.drafts)) {
      agents[agent] = {
        model: value.model,
        effort: value.effort,
        ...(agentSupportsUltracode(agent) ? { ultracode: value.ultracode } : {}),
        ...(agentSupportsPlanMode(agent) ? { planMode: value.planMode } : {}),
      }
    }
    setSavingDefaults(true)
    setSectionErrors((current) => ({ ...current, defaults: `` }))
    void trpc.devices.setLaunchDefaults
      .mutate({
        deviceId: snapshot.deviceId,
        launchDefaults: { defaultAgent: snapshot.defaultAgentDraft, agents },
      })
      .then((result) => {
        const stamp = result.launchDefaultsUpdatedAt
        const parsed = stamp ? Date.parse(stamp) : Number.NaN
        if (!Number.isNaN(parsed)) sentDefaultsStampRef.current = parsed
      })
      .catch((error) => {
        latest.current.defaultsPending = true
        setDefaultsPending(true)
        setSectionErrors((current) => ({
          ...current,
          defaults: trpcErrorMessage(
            error,
            `That didn't go through. Try again.`
          ),
        }))
      })
      .finally(() => setSavingDefaults(false))
  }

  const scheduleName = () => {
    latest.current.namePending = true
    setNamePending(true)
    if (nameTimerRef.current) clearTimeout(nameTimerRef.current)
    nameTimerRef.current = setTimeout(() => flushName(), NAME_DEBOUNCE_MS)
  }

  const scheduleDefaults = () => {
    latest.current.defaultsPending = true
    setDefaultsPending(true)
    if (defaultsTimerRef.current) clearTimeout(defaultsTimerRef.current)
    defaultsTimerRef.current = setTimeout(
      () => flushDefaults(),
      DEFAULTS_DEBOUNCE_MS
    )
  }

  const patchDraft = (patch: Partial<AgentDraft>) => {
    setDrafts((current) => ({
      ...current,
      [agentTab]: {
        ...(current[agentTab] ?? agentSeed(agentTab, null)),
        ...patch,
      },
    }))
    scheduleDefaults()
  }

  // Closing (or unmounting) writes whatever is still pending — the tRPC
  // promise outlives the component, so fire-and-forget is enough.
  useEffect(() => {
    if (!open) return
    return () => {
      flushName()
      flushDefaults()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // ── Worktree commands (durable queue + poll while open) ──────────────────
  const [tracked, setTracked] = useState<TrackedCommand[]>([])
  const [removeTarget, setRemoveTarget] =
    useState<SyncedDeviceWorktree | null>(null)
  // EXP-484: a finished command's `result`, kept per key AFTER the tracked
  // entry is dropped — `agent_login` completes EARLY with the sign-in URL,
  // which is the whole point of the round trip.
  const [commandResults, setCommandResults] = useState<Record<string, string>>(
    {}
  )
  const [switchTarget, setSwitchTarget] = useState<string | null>(null)

  const commandKey = (worktree: SyncedDeviceWorktree) =>
    `${worktree.repoFullName} ${worktree.branch}`

  const queueCommand = async (
    key: string,
    input:
      | { kind: `worktree_prune` }
      | { kind: `worktree_remove`; repoFullName: string; branch: string }
      | { kind: `agent_login`; agent: string; switch: boolean }
  ) => {
    if (!deviceId) return
    setSectionErrors((current) => ({ ...current, [key]: `` }))
    setCommandResults((current) => {
      if (!(key in current)) return current
      const next = { ...current }
      delete next[key]
      return next
    })
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
          // EXP-484: capture the payload BEFORE the tracked entry goes — a
          // login's whole answer (the sign-in URL) lives in `result`, and
          // only a `done` row ever carries one. Failures keep travelling
          // through `sectionErrors` like every other command.
          if (result.status === `done` && result.result) {
            const text = result.result
            setCommandResults((current) => ({ ...current, [command.key]: text }))
          }
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
    // Leading tick (EXP-490): with the relay nudge an online machine finishes
    // a remove in ~1-2s — a trailing-only interval added a guaranteed 2s.
    void tick()
    const interval = setInterval(() => void tick(), online ? 2_000 : 8_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [open, tracked, online])

  // A tracked remove whose row vanished from the synced inventory is done —
  // the Electric delta usually beats the next getCommand poll, so the spinner
  // clears on the earliest signal. Prune stays poll-only: its outcome is a
  // summary, not a specific row.
  useEffect(() => {
    if (tracked.length === 0) return
    const present = new Set(worktrees.map((worktree) => commandKey(worktree)))
    setTracked((current) =>
      current.filter(
        (command) => command.key === `prune` || present.has(command.key)
      )
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [worktrees])

  const pendingKey = (key: string) =>
    tracked.some((command) => command.key === key)

  const queueAgentLogin = (agent: string, switchAccount: boolean) =>
    void queueCommand(agentLoginKey(agent), {
      kind: `agent_login`,
      agent,
      switch: switchAccount,
    })

  const startAgentLogin = (agent: string, switchAccount: boolean) => {
    // `codex logout` revokes the token SERVER-side — switching accounts is
    // not a local-only act, so it asks first. Claude's is local.
    if (switchAccount && agent === `codex`) {
      setSwitchTarget(agent)
      return
    }
    queueAgentLogin(agent, switchAccount)
  }

  const dirtyLabel = (dirty: string): string | null =>
    dirty === `tracked`
      ? `uncommitted changes`
      : dirty === `untracked`
        ? `untracked files`
        : null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* EXP-686: no description — the machine's name is already the row you
          opened this from, and repeating it pushed the settings down a line
          on every client. `aria-describedby={undefined}` keeps Radix from
          pointing at a description that no longer exists. */}
      <DialogContent
        mobile="sheet-full"
        className="gap-4 sm:max-h-[85dvh] sm:max-w-lg"
        aria-describedby={undefined}
        // EXP-698: Radix autofocuses the first field and SELECTS its text, so
        // the Name row opened as a white selection block filling the row (and
        // the phone keyboard shot up over the sheet). The sheet is a list of
        // settings, not a form to type into — focus stays on the panel until
        // the field is tapped.
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>Device settings</DialogTitle>
        </DialogHeader>
        {/* EXP-694: one inset-grouped card stack — the same rows, in the same
            order, as the iOS/Android device sheets. 8px between groups. */}
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
          {/* ── Name ─────────────────────────────────────────────────── */}
          <GlassGroup>
            <GlassInputRow
              id="device-settings-name"
              label="Name"
              value={nameDraft}
              maxLength={255}
              trailing={
                savingName ? (
                  <LoaderCircle className="size-3 shrink-0 animate-spin text-muted-foreground" />
                ) : null
              }
              onChange={(event) => {
                setNameDraft(event.target.value)
                scheduleName()
              }}
              onFocus={() => {
                nameFocusedRef.current = true
              }}
              onBlur={() => {
                nameFocusedRef.current = false
                // A rename that arrived while the field was focused was
                // deliberately skipped — catch up unless an edit is owed.
                const hadPending = latest.current.namePending
                flushName()
                if (!hadPending && row) setNameDraft(row.label)
              }}
              onKeyDown={(event) => {
                if (event.key === `Enter`) flushName()
              }}
            />
          </GlassGroup>
          {sectionErrors.name && (
            <p className="px-1 text-xs text-destructive">
              {sectionErrors.name}
            </p>
          )}

          {/* ── Default machine (EXP-622) ────────────────────────────── */}
          <GlassGroup>
            <GlassToggleRow
              id="device-settings-default"
              label="Default device"
              checked={row?.isDefault ?? false}
              onCheckedChange={(checked) => void setDefault(checked)}
              disabled={busySection !== null}
            />
          </GlassGroup>
          {sectionErrors.default && (
            <p className="px-1 text-xs text-destructive">
              {sectionErrors.default}
            </p>
          )}

          {/* ── Sharing (server machines only, EXP-432) ───────────────── */}
          {kind === `server` && (
            <>
              <GlassGroup>
                <GlassPickerRow
                  label="Shared with"
                  value={row?.sharedTeamId ?? NOT_SHARED}
                  onValueChange={(value) =>
                    void setShared(value === NOT_SHARED ? null : value)
                  }
                  disabled={busySection !== null}
                  options={[
                    { value: NOT_SHARED, label: `Not shared` },
                    ...teams.map((team) => ({
                      value: team.id,
                      label: team.name,
                    })),
                  ]}
                />
              </GlassGroup>
              <p className="px-1 text-xs text-muted-foreground">
                Teammates of the shared team can start coding sessions on this
                machine.
              </p>
              {sectionErrors.sharing && (
                <p className="px-1 text-xs text-destructive">
                  {sectionErrors.sharing}
                </p>
              )}
            </>
          )}

          {/* ── Agent defaults (server-authoritative, EXP-481) ────────── */}
          {(!online || savingDefaults) && (
            <div className="flex items-center justify-between px-1">
              {!online ? (
                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                  <OfflineIcon className="size-3" />
                  Applies when the device comes online.
                </span>
              ) : (
                <span />
              )}
              {savingDefaults && (
                <LoaderCircle className="size-3 animate-spin text-muted-foreground" />
              )}
            </div>
          )}
          <GlassGroup>
            <GlassPickerRow
              label="Default agent"
              value={defaultAgentDraft}
              onValueChange={(value) => {
                setDefaultAgentDraft(value)
                scheduleDefaults()
              }}
              options={editorAgents.map((agent) => ({
                value: agent,
                label: AGENT_LABELS[agent] ?? agent,
              }))}
            />
          </GlassGroup>
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
            /* EXP-688: who this agent is signed in as on this machine, and
               what it has spent — under its OWN tab, not a section apart.
               EXP-694: rendered as that card's closing rows. */
            renderAgentFooter={(agent) => (
              <AgentAccountBlock
                agent={agent}
                row={row}
                online={online}
                canAgentLogin={deviceCanAgentLogin({ caps: row?.caps ?? [] })}
                now={now}
                error={sectionErrors[agentLoginKey(agent)] ?? ``}
                pending={pendingKey(agentLoginKey(agent))}
                result={commandResults[agentLoginKey(agent)] ?? null}
                onLogin={startAgentLogin}
              />
            )}
          />
          {sectionErrors.defaults && (
            <p className="px-1 text-xs text-destructive">
              {sectionErrors.defaults}
            </p>
          )}

          {/* ── Worktrees (reported inventory + durable commands) ─────── */}
          <GlassSectionHeader
            label="Worktrees"
            className="pt-2"
            trailing={
              /* EXP-688: icon only — the label repeated the section it sits
                 in, and the row reads as a heading with an action again. */
              <Button
                variant="ghost"
                className="h-5 w-5 p-0 text-muted-foreground"
                aria-label="Prune merged worktrees"
                title="Prune merged worktrees"
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
              </Button>
            }
          />
          {!online && (worktrees.length > 0 || pendingKey(`prune`)) && (
            <p className="px-1 pb-1 text-xs text-muted-foreground">
              This machine is offline — queued changes run when it comes
              online.
            </p>
          )}
          {sectionErrors.prune && (
            <p className="px-1 pb-1 text-xs text-destructive">
              {sectionErrors.prune}
            </p>
          )}
          <GlassGroup>
            {worktrees.length === 0 ? (
              <p className="px-4 py-3 text-xs text-muted-foreground">
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
                    className="flex flex-col gap-0.5 px-4 py-3"
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
                        <span className="shrink-0 rounded-sm border border-glass-stroke-card px-1 text-[10px] text-muted-foreground">
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
          </GlassGroup>
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

        <AlertDialog
          open={switchTarget !== null}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) setSwitchTarget(null)
          }}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Switch Codex account</AlertDialogTitle>
              <AlertDialogDescription>
                Codex logout revokes the token server-side; you'll sign in
                again on that machine.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  const target = switchTarget
                  setSwitchTarget(null)
                  if (target) queueAgentLogin(target, true)
                }}
              >
                Sign out and sign in
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}
