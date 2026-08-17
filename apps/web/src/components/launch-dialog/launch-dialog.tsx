import { useEffect, useMemo, useRef, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle, MonitorUp } from "lucide-react"
import { contract } from "@exp/domain-contract"
import type { CodingSession, Issue, SyncedDeviceWorktree } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import {
  actionCollection,
  codingSessionCollection,
  deviceWorktreeCollection,
  issueCollection,
} from "@/lib/collections"
import {
  BUILTIN_FIX_CONFLICTS_ID,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import { missingRequiredInputs, buildInputsPayload } from "@/lib/action-inputs"
import {
  agentSeed,
  agentSupportsPlanMode,
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
  DEFAULT_LAUNCH_AGENT,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"
import {
  deviceAgentIds,
  deviceAgentLaunchDefaults,
  deviceCanFixConflicts,
  deviceCanResume,
  deviceCanRunActionInputs,
  deviceCanRunActions,
  deviceDefaultAgent,
  deviceHasRunnableAgent,
  deviceIsOnline,
  resumeWorktree,
  type SteerDevice,
} from "@/lib/steer-devices"
import type { RemoteStartAction } from "@/hooks/use-remote-start"
import type {
  ActionRepoOption,
  TeamAction,
} from "@/components/action-editor-dialog"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  IssuesPane,
  MAX_ISSUES_PER_RUN,
} from "@/components/launch-dialog/issues-pane"
import { ActionsPane } from "@/components/launch-dialog/actions-pane"
import {
  CLI_DEFAULT_EFFORT,
  LaunchOptionsPane,
} from "@/components/launch-dialog/launch-options-pane"

// The unified launch dialog (EXP-257) — Issues | Actions tabs over ONE
// shared options cluster, the web twin of the desktop IDE's launcher.
// Issues tab (EXP-106): a searchable multi-issue picker — 1 checked issue
// starts a plain single-issue session; 2+ start a BATCH session on one pushed
// branch. While the user hasn't touched a Checkbox / Select, crossing to a
// batch flips ultracode ON / plan OFF, and dropping back restores the
// device's defaults. Actions tab (EXP-253/EXP-257): a single-select action
// list (the builtin "Fix merge conflicts" pinned first; "Create action"
// lives in its own dedicated dialog since EXP-431) plus the action's typed
// input fields; action runs take the FULL option set on any agent the device
// advertised.
//
// EXP-437: the options seed from the SELECTED DEVICE's advertised per-agent
// launch defaults (that machine's Settings → Agents configuration) — on
// settle after open, on every device switch, and on agent tab switches. A
// device that advertises nothing (older desktop build) seeds static contract
// defaults; nothing is persisted browser-side anymore.

/** The resolved dialog choices sent with `steer.startSession` — the same shape
 * the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

export type LaunchTab = `issues` | `actions`

// Only issues in a state worth coding are offered (mirrors the desktop picker).
// EXP-314: deliberately keyed on the dual-written ANCHOR enum, not on status
// rows — a custom backlog/unstarted/started status anchors into this set
// automatically, so custom statuses need no extra gating here.
const CODEABLE_STATUSES = new Set<string>([
  `backlog`,
  `todo`,
  `in_progress`,
  `in_review`,
])
// Cap the unchecked search results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

// EXP-349: an action bound to a repository pre-fills its repo-typed inputs
// with that repo — the picker showing "None" while the action runs in its
// bound repo anyway read as a misconfiguration. The user can still re-pick
// or clear the field.
const repoInputSeed = (action: TeamAction): Record<string, string> => {
  if (!action.repositoryId) return {}
  const seed: Record<string, string> = {}
  for (const def of action.inputs) {
    if (def.type === `repo`) seed[def.key] = action.repositoryId
  }
  return seed
}

export function LaunchDialog({
  open,
  onOpenChange,
  devices,
  starting,
  teamId,
  initialTab,
  initialIssueIds,
  initialDeviceId,
  initialActionId,
  initialPrIssueId,
  onStartIssues,
  onRunAction,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  devices: SteerDevice[]
  starting: boolean
  teamId: string
  /** Tab to open on (default issues). */
  initialTab?: LaunchTab
  /** Issues to pre-check when the dialog opens (e.g. the issue detail's issue). */
  initialIssueIds?: string[]
  /** Device to pre-select — wins over the first capable desktop. */
  initialDeviceId?: string
  /** Action to pre-select when opening on the Actions tab. */
  initialActionId?: string
  /**
   * Pre-pick the selected action's `pr` input (EXP-323 — the conflict-recovery
   * entry points hand over the issue their surface acts on; ANY issue linked
   * to the PR resolves).
   */
  initialPrIssueId?: string
  onStartIssues: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => void
  onRunAction: (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => void
}) {
  const [tab, setTab] = useState<LaunchTab>(`issues`)
  const [agent, setAgent] = useState<string>(contract.codingAgent.values[0])
  const [model, setModel] = useState(contract.codingModel.values[0])
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  const [skipPermissions, setSkipPermissions] = useState(false)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  const [search, setSearch] = useState(``)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  // EXP-481: "Resume previous session" — default ON whenever it first becomes
  // eligible (reset on open); a manual toggle simply sticks, since nothing
  // ever re-sets it after open.
  const [resume, setResume] = useState(true)
  // Set once the user overrides any Switch / Select — freezes the per-mode
  // defaults so a later selection-count crossing won't stomp their choice.
  const touchedRef = useRef(false)
  // EXP-437: the deviceId whose launch defaults last seeded the options —
  // the 15s devices re-poll must not stomp in-dialog edits, but an actual
  // device change (explicit switch, or a re-settle after the picked machine
  // dropped offline) reseeds.
  const seededDeviceRef = useRef<string | null>(null)

  // Actions tab state (EXP-257).
  const [actionSearch, setActionSearch] = useState(``)
  const [selectedActionId, setSelectedActionId] = useState<string | null>(null)
  const [inputValues, setInputValues] = useState<Record<string, string>>({})
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  // Last action id whose repo inputs were seeded (EXP-349) — the latch keeps
  // a manual re-pick (including clearing to "None") from being re-seeded when
  // the Electric actions rows update.
  const seededRepoActionId = useRef<string | null>(null)

  // Repos on OPEN (tRPC-only surface); actions ride the Electric shape.
  useEffect(() => {
    if (!open) return
    let active = true
    trpc.repositories.list
      .query({ teamId })
      .then(
        (rows) =>
          active &&
          setRepos(rows.map((r) => ({ id: r.id, fullName: r.fullName })))
      )
      .catch(() => {})
    return () => {
      active = false
    }
  }, [open, teamId])

  // Live synced actions (EXP-268 — the body-less list projection); the
  // fix-conflicts builtin (not a DB row) pinned FIRST, the rest re-apply the
  // server's ordering (sortOrder, name). "Create action" is deliberately not
  // offered here (EXP-431 — it has its own dialog on the Agents page).
  const { data: syncedActionRows } = useLiveQuery(
    (query) =>
      open
        ? query
            .from({ a: actionCollection })
            .where(({ a }) => eq(a.teamId, teamId))
        : undefined,
    [open, teamId]
  )
  const actions = useMemo<TeamAction[] | null>(() => {
    if (!open || syncedActionRows === undefined) return null
    const rows = [...syncedActionRows]
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((row) => ({ ...row, builtin: false as const }))
    return [builtinFixConflictsAction(teamId), ...rows]
  }, [open, teamId, syncedActionRows])
  const selectedAction =
    (actions ?? []).find((action) => action.id === selectedActionId) ?? null
  const inputDefs = selectedAction?.inputs ?? []

  // Codeable issues live in boards that HAVE a repo — coding gates on repo
  // presence (board type is irrelevant). Sorted ids keep the dep string
  // stable so the same set never churns the query.
  const boards = useTeamBoards(teamId)
  const repoBoardIds = useMemo(() => {
    const ids = boards.filter((p) => p.repositoryId).map((p) => p.id)
    ids.sort()
    return ids
  }, [boards])

  const { data: issueRows } = useLiveQuery(
    (query) =>
      open && repoBoardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, repoBoardIds))
        : undefined,
    [open, repoBoardIds.join(`,`)]
  )

  const { data: runningRows } = useLiveQuery(
    (query) =>
      open
        ? query.from({ s: codingSessionCollection }).where(({ s }) =>
            and(
              eq(s.teamId, teamId),
              // in_review/merged terminals are still alive and occupy the
              // issue's worktree (EXP-194/EXP-358) — they block a restart
              // like running ones.
              inArray(s.status, [`running`, `in_review`, `merged`])
            )
          )
        : undefined,
    [open, teamId]
  )

  // Staleness guard (EXP-153): a heartbeat-dead row must not keep
  // its issue blocked from a fresh start.
  const now = useNow()
  const runningIssueIds = useMemo(() => {
    const set = new Set<string>()
    for (const s of (runningRows ?? []) as CodingSession[]) {
      if (s.issueId && !isCodingSessionStale(s.updatedAt, now))
        set.add(s.issueId)
    }
    return set
  }, [runningRows, now])

  // Every repo-board issue, for looking up already-checked rows (a pre-checked
  // issue may not itself be "codeable", e.g. a done issue started from detail).
  const allById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((i) => [i.id, i])),
    [issueRows]
  )

  // Fresh, unchecked, codeable, not-already-running candidates for the search.
  const eligible = useMemo(
    () =>
      ((issueRows ?? []) as Issue[])
        .filter(
          (issue) =>
            CODEABLE_STATUSES.has(issue.status) &&
            !runningIssueIds.has(issue.id)
        )
        .sort(
          (a, b) =>
            new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
        ),
    [issueRows, runningIssueIds]
  )

  // Checked rows pin to the top; search matches follow.
  const checkedIssues = useMemo(
    () =>
      [...selected]
        .map((id) => allById.get(id))
        .filter((i): i is Issue => Boolean(i))
        .sort(
          (a, b) =>
            new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
        ),
    [selected, allById]
  )

  // A batch run is ONE repository (the server enforces same-repo across the
  // linked PR) — resolve each checked issue's repo via its board and block a
  // cross-repo selection client-side.
  const boardRepoById = useMemo(
    () => new Map(boards.map((p) => [p.id, p.repositoryId])),
    [boards]
  )
  const checkedRepoIds = useMemo(() => {
    const set = new Set<string>()
    for (const issue of checkedIssues) {
      const repoId = boardRepoById.get(issue.boardId)
      if (repoId) set.add(repoId)
    }
    return set
  }, [checkedIssues, boardRepoById])

  const searchMatches = useMemo(() => {
    const q = search.trim().toLowerCase()
    const candidates = eligible.filter((i) => !selected.has(i.id))
    const filtered = q
      ? candidates.filter(
          (i) =>
            i.identifier.toLowerCase().includes(q) ||
            i.title.toLowerCase().includes(q)
        )
      : candidates
    return filtered.slice(0, MAX_UNCHECKED)
  }, [eligible, selected, search])

  // Seed the tab, selection + launch options on OPEN only — a desktop
  // connecting mid-dialog (the device effect below) must never wipe the picker.
  useEffect(() => {
    if (!open) return
    setTab(initialTab ?? `issues`)
    const initial = new Set(initialIssueIds ?? [])
    setSelected(initial)
    setSearch(``)
    setActionSearch(``)
    setSelectedActionId(initialActionId ?? null)
    setInputValues({})
    seededRepoActionId.current = null
    setDeviceId(initialDeviceId ?? null)
    setResume(true)
    touchedRef.current = false
    // Static contract defaults until a device settles — the device-seed
    // effect below overlays the selected machine's advertised defaults
    // (EXP-437; its latch is reset here so a reopen reseeds).
    seededDeviceRef.current = null
    const seed = agentSeed(DEFAULT_LAUNCH_AGENT, null)
    setAgent(DEFAULT_LAUNCH_AGENT)
    setModel(seed.model)
    setEffortValue(CLI_DEFAULT_EFFORT)
    setSkipPermissions(seed.skipPermissions)
    // A pre-checked batch (2+) opens with the batch defaults (ultracode ON /
    // plan OFF).
    if (initial.size >= 2) {
      setUltracode(agentSupportsUltracode(DEFAULT_LAUNCH_AGENT))
      setPlanMode(false)
    } else {
      setUltracode(seed.ultracode)
      setPlanMode(seed.planMode)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Seed repo-typed inputs from the selected action's bound repository, once
  // per selection (EXP-349). Dialog-level rather than in `selectAction`
  // because the initially-selected action's Electric row may sync in after
  // open; the merge is current-wins so the PrInputField seed (fix-conflicts
  // path) and any already-typed values are never stomped.
  useEffect(() => {
    if (!open || !selectedActionId) return
    if (seededRepoActionId.current === selectedActionId) return
    const action = (actions ?? []).find((a) => a.id === selectedActionId)
    // Row not synced yet — leave the latch unset so the arrival retries.
    if (!action) return
    seededRepoActionId.current = selectedActionId
    const seed = repoInputSeed(action)
    if (Object.keys(seed).length > 0) {
      setInputValues((current) => ({ ...seed, ...current }))
    }
  }, [open, selectedActionId, actions])

  // Per-tab device candidates: Issues offers every online desktop; Actions
  // only actions-capable ones, tightened to action-inputs-capable when the
  // selected action is the builtin or declares inputs (the server enforces
  // the same caps at start time).
  const needsInputsCap = Boolean(
    selectedAction && (selectedAction.builtin || inputDefs.length > 0)
  )
  // The "Fix merge conflicts" builtin needs its own cap on top (EXP-259) —
  // filter here so an outdated desktop can't be picked and fail after submit.
  const needsFixConflictsCap = selectedAction?.id === BUILTIN_FIX_CONFLICTS_ID
  const candidateDevices = useMemo(() => {
    // EXP-403: the registry lists offline machines too — only online ones
    // are startable (the relay would 404 the rest with device_offline).
    // EXP-409: a machine whose every agent is signed out is equally
    // unstartable — the My machines list carries the "sign in" reason.
    const online = devices
      .filter(deviceIsOnline)
      .filter(deviceHasRunnableAgent)
    return tab === `issues`
      ? online
      : online.filter(
          (candidate) =>
            deviceCanRunActions(candidate) &&
            (!needsInputsCap || deviceCanRunActionInputs(candidate)) &&
            (!needsFixConflictsCap || deviceCanFixConflicts(candidate))
        )
  }, [devices, tab, needsInputsCap, needsFixConflictsCap])

  // Settle the device on open + whenever the candidate list changes (tab
  // switch, action selection, a desktop connecting mid-dialog); a still-valid
  // current choice is kept, else the first candidate wins.
  useEffect(() => {
    if (!open) return
    setDeviceId((current) =>
      current && candidateDevices.some((d) => d.deviceId === current)
        ? current
        : (candidateDevices[0]?.deviceId ?? null)
    )
  }, [open, candidateDevices])

  const markTouched = () => {
    touchedRef.current = true
  }

  const device =
    candidateDevices.find((candidate) => candidate.deviceId === deviceId) ??
    candidateDevices[0]

  const toggleIssue = (id: string) => {
    const next = new Set(selected)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    setSelected(next)
    // Apply per-mode defaults on a 1↔2 crossing, but only while untouched.
    const wasBatch = selected.size >= 2
    const isBatch = next.size >= 2
    if (wasBatch !== isBatch && !touchedRef.current) {
      if (isBatch) {
        setUltracode(agentSupportsUltracode(agent))
        setPlanMode(false)
      } else {
        // Back to a single issue: restore the device's defaults (EXP-437).
        const seed = agentSeed(agent, deviceAgentLaunchDefaults(device, agent))
        setUltracode(seed.ultracode)
        setPlanMode(seed.planMode)
      }
    }
  }

  const selectAction = (actionId: string) => {
    if (actionId === selectedActionId) return
    setSelectedActionId(actionId)
    // A different action has a different input schema — stale values must
    // not leak into the new one's payload.
    setInputValues({})
  }

  // Switching the agent tab re-seeds model/effort/toggles to the SELECTED
  // DEVICE's defaults for that agent (EXP-437; static when it advertises
  // none), capability-clamped — the same reseed the desktop dialog does.
  const switchAgent = (next: string) => {
    if (next === agent) return
    markTouched()
    setAgent(next)
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setSkipPermissions(seed.skipPermissions)
    // The batch posture survives an agent switch (ultracode ON / plan OFF).
    if (selected.size >= 2) {
      setUltracode(agentSupportsUltracode(next))
      setPlanMode(false)
    } else {
      setUltracode(seed.ultracode)
      setPlanMode(seed.planMode)
    }
  }

  // EXP-437: seed the launch options from the selected device's advertised
  // per-agent defaults — once a device settles after open, and again on
  // every actual device change (the ref latch skips same-device re-polls).
  useEffect(() => {
    if (!open || !device) return
    if (seededDeviceRef.current === device.deviceId) return
    seededDeviceRef.current = device.deviceId
    const available = deviceAgentIds(device)
    const next =
      deviceDefaultAgent(device) ??
      (available.includes(agent)
        ? agent
        : (available[0] ?? DEFAULT_LAUNCH_AGENT))
    const seed = agentSeed(next, deviceAgentLaunchDefaults(device, next))
    setAgent(next)
    setModel(seed.model)
    setEffortValue(seed.effort === `` ? CLI_DEFAULT_EFFORT : seed.effort)
    setSkipPermissions(seed.skipPermissions)
    // The batch override still wins over the device defaults (mirrors open).
    if (selected.size >= 2) {
      setUltracode(agentSupportsUltracode(next))
      setPlanMode(false)
    } else {
      setUltracode(seed.ultracode)
      setPlanMode(seed.planMode)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId])

  // EXP-201: only agents the chosen device advertised are offerable; a
  // device change re-clamps a now-unavailable selection.
  const availableAgents = deviceAgentIds(device)
  const availableAgentsKey = availableAgents.join(`,`)
  useEffect(() => {
    if (!open) return
    if (!availableAgents.includes(agent)) {
      switchAgent(availableAgents[0] ?? `claude`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, availableAgentsKey, agent])

  const count = selected.size
  const isBatch = count >= 2
  const overCap = count > MAX_ISSUES_PER_RUN
  const spansRepos = checkedRepoIds.size > 1
  const blocked = overCap || spansRepos

  // EXP-481: the selected device's synced worktree inventory — offers
  // "Resume previous session" when exactly one issue is checked and that
  // machine reports a worktree for it (matching the chosen agent's resume
  // marker). Persisted data, so the offer works even from a stale report —
  // the device-side launcher degrades a missing worktree gracefully.
  const { data: worktreeRows } = useLiveQuery(
    (query) =>
      open ? query.from({ w: deviceWorktreeCollection }) : undefined,
    [open]
  )
  const soleIssue =
    tab === `issues` && count === 1
      ? allById.get([...selected][0]!)
      : undefined
  const resumeCandidate =
    soleIssue && device && deviceCanResume(device)
      ? resumeWorktree(
          (worktreeRows ?? []) as SyncedDeviceWorktree[],
          device.rowId,
          soleIssue.identifier,
          agent
        )
      : null
  const resumeActive = resume && resumeCandidate !== null

  const missingInputs =
    tab === `actions` ? missingRequiredInputs(inputDefs, inputValues) : []
  const submitBlocked =
    tab === `issues`
      ? count === 0 || blocked
      : !selectedAction || missingInputs.length > 0

  const submit = () => {
    if (!device || submitBlocked) return
    const options: StartCodingOptions = {
      agent,
      model,
      effort: effortValue === CLI_DEFAULT_EFFORT ? `` : effortValue,
      ultracode: ultracode && agentSupportsUltracode(agent),
      // A resumed session never re-enters plan mode (EXP-481, mirrors the
      // desktop launcher's clamp).
      planMode: planMode && agentSupportsPlanMode(agent) && !resumeActive,
      skipPermissions: skipPermissions && agentSupportsSkipPermissions(agent),
      ...(resumeActive ? { resume: true } : {}),
    }
    if (tab === `issues`) {
      onStartIssues(device, options, [...selected])
      return
    }
    if (!selectedAction) return
    onRunAction(
      device,
      {
        id: selectedAction.id,
        name: selectedAction.name,
        teamId: selectedAction.teamId,
      },
      options,
      buildInputsPayload(inputDefs, inputValues)
    )
  }

  const pickerRows = [...checkedIssues, ...searchMatches]

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* The header/tabs/footer stay anchored (the ui/dialog base is a flex
          column) while only the BODY takes the height budget — it is the
          flex-1 min-h-0 row. On mobile the dialog is a full-screen page
          (EXP-255 — the ui/dialog base) and the body stacks vertically,
          scrolling as one region; from `sm` up the body splits into two
          columns — issue/action picker left, launch options right — where
          ONLY the picker list scrolls, so the dialog never shows nested
          scrollbars. */}
      <DialogContent className="gap-3 sm:max-h-[85dvh] sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            {tab === `actions` ? `Run action` : `Start coding`}
          </DialogTitle>
        </DialogHeader>
        <Tabs value={tab} onValueChange={(value) => setTab(value as LaunchTab)}>
          <TabsList className="w-full">
            <TabsTrigger value="issues" className="flex-1">
              Issues
            </TabsTrigger>
            <TabsTrigger value="actions" className="flex-1">
              Actions
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto sm:grid sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:gap-5 sm:overflow-y-visible">
          {tab === `issues` ? (
            <IssuesPane
              search={search}
              onSearchChange={setSearch}
              rows={pickerRows}
              selected={selected}
              onToggle={toggleIssue}
              count={count}
              overCap={overCap}
              spansRepos={spansRepos}
              blocked={blocked}
            />
          ) : (
            <ActionsPane
              actions={actions}
              search={actionSearch}
              onSearchChange={setActionSearch}
              selectedActionId={selectedActionId}
              onSelect={selectAction}
              inputValues={inputValues}
              onInputChange={(key, value) =>
                setInputValues((current) => ({ ...current, [key]: value }))
              }
              repos={repos}
              teamId={teamId}
              seedPrIssueId={initialPrIssueId}
            />
          )}

          <LaunchOptionsPane
            devices={candidateDevices}
            device={device}
            onDeviceChange={setDeviceId}
            noDeviceNote={
              tab === `actions`
                ? needsFixConflictsCap
                  ? `No desktop can fix merge conflicts yet. Update the Exponential desktop app.`
                  : needsInputsCap
                    ? `No capable desktop online. This action needs a desktop app new enough to run action inputs.`
                    : `No actions-capable desktop online. Open (or update) the Exponential desktop app.`
                : `No desktop online. Open the Exponential desktop app to start coding.`
            }
            agent={agent}
            availableAgents={availableAgents}
            onAgentChange={switchAgent}
            model={model}
            onModelChange={(value) => {
              markTouched()
              setModel(value)
            }}
            effortValue={effortValue}
            onEffortChange={(value) => {
              markTouched()
              setEffortValue(value)
            }}
            ultracode={ultracode}
            onUltracodeChange={(value) => {
              markTouched()
              setUltracode(value)
            }}
            planMode={planMode}
            onPlanModeChange={(value) => {
              markTouched()
              setPlanMode(value)
            }}
            planModeHidden={resumeActive}
            skipPermissions={skipPermissions}
            onSkipPermissionsChange={(value) => {
              markTouched()
              setSkipPermissions(value)
            }}
            resumeRow={
              resumeCandidate
                ? {
                    checked: resume,
                    onChange: setResume,
                    identifier: soleIssue!.identifier,
                    branch: resumeCandidate.branch,
                  }
                : null
            }
          />
        </div>
        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={starting}
          >
            Cancel
          </Button>
          <Button
            onClick={submit}
            disabled={starting || !device || submitBlocked}
          >
            {starting ? (
              <LoaderCircle className="animate-spin" />
            ) : (
              <MonitorUp />
            )}
            {tab === `actions`
              ? `Run action`
              : isBatch
                ? `Start batch (${count} issues)`
                : `Start coding`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
