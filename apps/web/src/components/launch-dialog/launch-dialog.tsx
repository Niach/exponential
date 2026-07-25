import { useEffect, useMemo, useRef, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { Loader2, MonitorUp } from "lucide-react"
import { contract } from "@exp/domain-contract"
import type { CodingSession, Issue } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import { missingRequiredInputs, buildInputsPayload } from "@/lib/action-inputs"
import {
  agentSupportsPlanMode,
  agentSupportsSkipPermissions,
  agentSupportsUltracode,
  defaultModelFor,
  readCodingLaunchPrefs,
  rememberCodingLaunchPrefs,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"
import {
  deviceAgentIds,
  deviceCanRunActionInputs,
  deviceCanRunActions,
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
  DialogDescription,
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
  AGENT_LABELS,
  CLI_DEFAULT_EFFORT,
  LaunchOptionsPane,
} from "@/components/launch-dialog/launch-options-pane"

// The unified launch dialog (EXP-257) — Issues | Actions tabs over ONE
// shared options cluster, the web twin of the desktop IDE's launcher.
// Issues tab (EXP-106): a searchable multi-issue picker — 1 checked issue
// starts a plain single-issue session; 2+ start a BATCH session on one pushed
// branch. Per-mode defaults track the desktop: while the user hasn't touched a
// Checkbox / Select, crossing to a batch flips ultracode ON / plan OFF, and
// dropping back re-seeds the remembered single-issue prefs. Actions tab
// (EXP-253/EXP-257): a single-select action list (the builtin "Create action"
// pinned first) plus the action's typed input fields; action runs take the
// FULL option set on any agent the device advertised. Single-issue and action
// submits persist the prefs; batch submits don't (batch defaults must not
// overwrite them).

/** The resolved dialog choices sent with `steer.startSession` — the same shape
 * the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

export type LaunchTab = `issues` | `actions`

// Only issues in a state worth coding are offered (mirrors the desktop picker).
const CODEABLE_STATUSES = new Set<string>([
  `backlog`,
  `todo`,
  `in_progress`,
  `in_review`,
])
// Cap the unchecked search results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50

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
  // Set once the user overrides any Switch / Select — freezes the per-mode
  // defaults so a later selection-count crossing won't stomp their choice.
  const touchedRef = useRef(false)

  // Actions tab state (EXP-257).
  const [actionSearch, setActionSearch] = useState(``)
  const [selectedActionId, setSelectedActionId] = useState<string | null>(null)
  const [inputValues, setInputValues] = useState<Record<string, string>>({})
  const [actionRows, setActionRows] = useState<TeamAction[] | null>(null)
  const [repos, setRepos] = useState<ActionRepoOption[]>([])

  // Fetch the team's actions + repos on OPEN (tRPC-only surfaces — neither is
  // an Electric shape).
  useEffect(() => {
    if (!open) return
    let active = true
    setActionRows(null)
    trpc.actions.list
      .query({ teamId })
      .then((res) => active && setActionRows(res.actions))
      .catch(() => active && setActionRows([]))
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

  // Builtin pinned FIRST by its flag (never by sort order); the rest keep the
  // server's ordering.
  const actions = useMemo(
    () =>
      actionRows
        ? [...actionRows].sort((a, b) => Number(b.builtin) - Number(a.builtin))
        : null,
    [actionRows]
  )
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
              // in_review terminals are still alive and occupy the issue's
              // worktree (EXP-194) — they block a restart like running ones.
              inArray(s.status, [`running`, `in_review`])
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
            issue.archivedAt == null &&
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
    setDeviceId(initialDeviceId ?? null)
    touchedRef.current = false
    const prefs = readCodingLaunchPrefs()
    setAgent(prefs.agent)
    setModel(prefs.model)
    setEffortValue(prefs.effort === `` ? CLI_DEFAULT_EFFORT : prefs.effort)
    setSkipPermissions(prefs.skipPermissions)
    // A pre-checked batch (2+) opens with the batch defaults (ultracode ON /
    // plan OFF); a single issue opens with the remembered prefs. Both stay
    // capability-clamped to the remembered agent (EXP-201).
    if (initial.size >= 2) {
      setUltracode(agentSupportsUltracode(prefs.agent))
      setPlanMode(false)
    } else {
      setUltracode(prefs.ultracode && agentSupportsUltracode(prefs.agent))
      setPlanMode(prefs.planMode && agentSupportsPlanMode(prefs.agent))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Per-tab device candidates: Issues offers every online desktop; Actions
  // only actions-capable ones, tightened to action-inputs-capable when the
  // selected action is the builtin or declares inputs (the server enforces
  // the same caps at start time).
  const needsInputsCap = Boolean(
    selectedAction && (selectedAction.builtin || inputDefs.length > 0)
  )
  const candidateDevices = useMemo(
    () =>
      tab === `issues`
        ? devices
        : devices.filter(
            (candidate) =>
              deviceCanRunActions(candidate) &&
              (!needsInputsCap || deviceCanRunActionInputs(candidate))
          ),
    [devices, tab, needsInputsCap]
  )

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
        const prefs = readCodingLaunchPrefs()
        setUltracode(prefs.ultracode && agentSupportsUltracode(agent))
        setPlanMode(prefs.planMode && agentSupportsPlanMode(agent))
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

  const device =
    candidateDevices.find((candidate) => candidate.deviceId === deviceId) ??
    candidateDevices[0]

  // Switching the agent tab re-seeds model/effort to the agent's own
  // defaults and clamps the capability toggles (EXP-201).
  const switchAgent = (next: string) => {
    if (next === agent) return
    markTouched()
    setAgent(next)
    setModel(defaultModelFor(next))
    setEffortValue(CLI_DEFAULT_EFFORT)
    if (!agentSupportsUltracode(next)) setUltracode(false)
    if (!agentSupportsPlanMode(next)) setPlanMode(false)
    if (!agentSupportsSkipPermissions(next)) setSkipPermissions(false)
  }

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
      planMode: planMode && agentSupportsPlanMode(agent),
      skipPermissions: skipPermissions && agentSupportsSkipPermissions(agent),
    }
    if (tab === `issues`) {
      // Persist prefs only for a single-issue launch — batch defaults
      // (ultracode ON / plan OFF) must not overwrite the remembered
      // single-issue prefs.
      if (count === 1) rememberCodingLaunchPrefs(options)
      onStartIssues(device, options, [...selected])
      return
    }
    if (!selectedAction) return
    rememberCodingLaunchPrefs(options)
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

  const agentLabel = AGENT_LABELS[agent] ?? agent
  const description =
    tab === `actions`
      ? selectedAction
        ? `Run "${selectedAction.name}" with ${agentLabel}${
            device ? ` on ${device.deviceLabel}` : ``
          }.`
        : `Pick an action to run on one of your desktops.`
      : isBatch
        ? `Launch one ${agentLabel} batch session across ${count} issues${
            device ? ` on ${device.deviceLabel}` : ``
          }.`
        : devices.length === 1 && device
          ? `Launch a ${agentLabel} session on ${device.deviceLabel}.`
          : `Launch a ${agentLabel} session on one of your desktops.`

  const pickerRows = [...checkedIssues, ...searchMatches]

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* The header/tabs/footer rows stay anchored while only the BODY row
          works with the height budget. On mobile the dialog is a full-screen
          page (EXP-255 — the ui/dialog base) and the body stacks vertically,
          scrolling as one region; from `sm` up the body splits into two
          columns — issue/action picker left, launch options right — where
          ONLY the picker list scrolls, so the dialog never shows nested
          scrollbars. */}
      <DialogContent className="grid-rows-[auto_auto_minmax(0,1fr)_auto] gap-3 sm:max-h-[85dvh] sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            {tab === `actions` ? `Run action` : `Start coding`}
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
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
        <div className="flex min-h-0 flex-col gap-3 overflow-y-auto sm:grid sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:gap-5 sm:overflow-y-visible">
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
            />
          )}

          <LaunchOptionsPane
            devices={candidateDevices}
            device={device}
            onDeviceChange={setDeviceId}
            noDeviceNote={
              tab === `actions`
                ? needsInputsCap
                  ? `No capable desktop online — this action needs a desktop app new enough to run action inputs.`
                  : `No actions-capable desktop online — open (or update) the Exponential desktop app.`
                : `No desktop online — open the Exponential desktop app to start coding.`
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
            skipPermissions={skipPermissions}
            onSkipPermissionsChange={(value) => {
              markTouched()
              setSkipPermissions(value)
            }}
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
            {starting ? <Loader2 className="animate-spin" /> : <MonitorUp />}
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
