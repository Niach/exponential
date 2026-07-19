import { useEffect, useMemo, useRef, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { Loader2, MonitorUp, Search } from "lucide-react"
import { contract } from "@exp/domain-contract"
import type { CodingSession, Issue } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import {
  readCodingLaunchPrefs,
  rememberCodingLaunchPrefs,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"

// The unified Start-coding dialog (EXP-106) — the web twin of the desktop IDE's
// ONE launcher: a searchable multi-issue picker plus Model / Effort selects,
// ultracode + plan-mode switches, and the device picker. 1 checked issue starts
// a plain single-issue session; 2+ start a BATCH session on one pushed branch.
// Per-mode defaults track the desktop: while the user hasn't touched a Switch /
// Select, crossing to a batch flips ultracode ON / plan OFF, and dropping back
// re-seeds the remembered single-issue prefs. Single-issue submits persist the
// prefs; batch submits don't (batch defaults must not overwrite them).

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
}

/** The resolved dialog choices sent with `steer.startSession` — the same shape
 * the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

// Radix Select forbids an empty-string item value; the blank "CLI default"
// effort rides this sentinel inside the dialog only.
const CLI_DEFAULT_EFFORT = `cli-default`

// Only issues in a state worth coding are offered (mirrors the desktop picker).
const CODEABLE_STATUSES = new Set<string>([
  `backlog`,
  `todo`,
  `in_progress`,
  `in_review`,
])
// Cap the unchecked search results so a huge board can't blow up the list.
const MAX_UNCHECKED = 50
// Hard cap per run — parity with the server zod cap (issueIds max 30) and the
// desktop launcher's MAX_ISSUES_PER_RUN. Beyond it the server would reject with
// a zod BAD_REQUEST whose `[`-prefixed message is discarded into a misleading
// "could not be delivered" toast, so block the submit here instead.
const MAX_ISSUES_PER_RUN = 30
// Above this, batches get a soft token-cost note (matches the native sheets).
const BATCH_COST_HINT_THRESHOLD = 6

// Display labels derive from the contract values (same rule as the iOS and
// Android sheets), so a new contract value can never render unlabeled.
function modelLabel(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}

function effortLabel(value: string): string {
  return value === `xhigh` ? `XHigh` : modelLabel(value)
}

export function StartCodingDialog({
  open,
  onOpenChange,
  devices,
  starting,
  teamId,
  initialIssueIds,
  initialDeviceId,
  onStart,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  devices: SteerDevice[]
  starting: boolean
  teamId: string
  /** Issues to pre-check when the dialog opens (e.g. the issue detail's issue). */
  initialIssueIds?: string[]
  /** Device to pre-select — wins over the first online desktop. */
  initialDeviceId?: string
  onStart: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => void
}) {
  const [model, setModel] = useState(contract.codingModel.values[0])
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  const [deviceId, setDeviceId] = useState<string | null>(null)
  const [search, setSearch] = useState(``)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  // Set once the user overrides any Switch / Select — freezes the per-mode
  // defaults so a later selection-count crossing won't stomp their choice.
  const touchedRef = useRef(false)

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
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) =>
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
      if (s.issueId && !isCodingSessionStale(s.updatedAt, now)) set.add(s.issueId)
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

  // Seed the selection + launch options on OPEN only — a desktop connecting
  // mid-dialog (the device effect below) must never wipe the picker.
  useEffect(() => {
    if (!open) return
    const initial = new Set(initialIssueIds ?? [])
    setSelected(initial)
    setSearch(``)
    touchedRef.current = false
    const prefs = readCodingLaunchPrefs()
    setModel(prefs.model)
    setEffortValue(prefs.effort === `` ? CLI_DEFAULT_EFFORT : prefs.effort)
    // A pre-checked batch (2+) opens with the batch defaults (ultracode ON /
    // plan OFF); a single issue opens with the remembered prefs.
    if (initial.size >= 2) {
      setUltracode(true)
      setPlanMode(false)
    } else {
      setUltracode(prefs.ultracode)
      setPlanMode(prefs.planMode)
    }
  }, [open])

  // Settle the device on open + whenever the device list changes; initialDeviceId
  // wins over the first online desktop.
  useEffect(() => {
    if (!open) return
    setDeviceId(initialDeviceId ?? devices[0]?.deviceId ?? null)
  }, [open, devices, initialDeviceId])

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
        setUltracode(true)
        setPlanMode(false)
      } else {
        const prefs = readCodingLaunchPrefs()
        setUltracode(prefs.ultracode)
        setPlanMode(prefs.planMode)
      }
    }
  }

  const device =
    devices.find((candidate) => candidate.deviceId === deviceId) ?? devices[0]

  const count = selected.size
  const isBatch = count >= 2
  const overCap = count > MAX_ISSUES_PER_RUN
  const spansRepos = checkedRepoIds.size > 1
  const blocked = overCap || spansRepos

  const submit = () => {
    if (!device || count === 0 || blocked) return
    const options: StartCodingOptions = {
      model,
      effort: effortValue === CLI_DEFAULT_EFFORT ? `` : effortValue,
      ultracode,
      planMode,
    }
    // Persist prefs only for a single-issue launch — batch defaults (ultracode
    // ON / plan OFF) must not overwrite the remembered single-issue prefs.
    if (count === 1) rememberCodingLaunchPrefs(options)
    onStart(device, options, [...selected])
  }

  const description = isBatch
    ? `Launch one Claude batch session across ${count} issues${
        device ? ` on ${device.deviceLabel}` : ``
      }.`
    : devices.length === 1 && device
      ? `Launch a Claude session on ${device.deviceLabel}.`
      : `Launch a Claude session on one of your desktops.`

  const pickerRows = [...checkedIssues, ...searchMatches]

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Start coding</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label>Issues</Label>
            <div className="flex items-center gap-2 rounded-md border border-border px-2.5">
              <Search className="size-4 shrink-0 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search issues…"
                className="h-9 border-none px-0 shadow-none focus-visible:ring-0"
              />
            </div>
            <div className="max-h-56 overflow-y-auto rounded-md border border-border">
              {pickerRows.length === 0 ? (
                <div className="px-3 py-6 text-center text-xs text-muted-foreground">
                  {search.trim()
                    ? `No issues match "${search}"`
                    : `No codeable issues in repo-backed boards.`}
                </div>
              ) : (
                pickerRows.map((issue) => {
                  const checked = selected.has(issue.id)
                  return (
                    <div
                      key={issue.id}
                      role="button"
                      tabIndex={0}
                      onClick={() => toggleIssue(issue.id)}
                      onKeyDown={(e) => {
                        if (e.key === `Enter` || e.key === ` `) {
                          e.preventDefault()
                          toggleIssue(issue.id)
                        }
                      }}
                      className="flex cursor-pointer items-center gap-2 border-b border-border/30 px-3 py-2 last:border-b-0 hover:bg-muted/50"
                    >
                      <Checkbox
                        checked={checked}
                        tabIndex={-1}
                        className="pointer-events-none"
                      />
                      <StatusIcon status={issue.status} className="size-4 shrink-0" />
                      <span className="shrink-0 font-mono text-xs text-muted-foreground">
                        {issue.identifier}
                      </span>
                      <span className="flex-1 truncate text-sm">{issue.title}</span>
                    </div>
                  )
                })
              )}
            </div>
            {count > 0 && (
              <p className="text-xs text-muted-foreground">
                {count} issue{count === 1 ? `` : `s`} selected
              </p>
            )}
            {overCap && (
              <p className="text-xs text-destructive">
                At most {MAX_ISSUES_PER_RUN} issues per run — split the batch.
              </p>
            )}
            {spansRepos && (
              <p className="text-xs text-destructive">
                Pick issues from a single repository per run.
              </p>
            )}
            {!blocked && count > BATCH_COST_HINT_THRESHOLD && (
              <p className="text-xs text-muted-foreground">
                Large batches are token-expensive.
              </p>
            )}
          </div>

          {devices.length > 1 && (
            <div className="space-y-2">
              <Label htmlFor="start-coding-device">Desktop</Label>
              <Select
                value={device?.deviceId ?? ``}
                onValueChange={setDeviceId}
              >
                <SelectTrigger id="start-coding-device" className="w-full">
                  <SelectValue placeholder="Select a desktop" />
                </SelectTrigger>
                <SelectContent>
                  {devices.map((candidate) => (
                    <SelectItem
                      key={candidate.deviceId}
                      value={candidate.deviceId}
                    >
                      {candidate.deviceLabel || candidate.deviceId}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label htmlFor="start-coding-model">Model</Label>
              <Select
                value={model}
                onValueChange={(value) => {
                  markTouched()
                  setModel(value)
                }}
              >
                <SelectTrigger id="start-coding-model" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {contract.codingModel.values.map((value) => (
                    <SelectItem key={value} value={value}>
                      {modelLabel(value)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="start-coding-effort">Effort</Label>
              <Select
                value={effortValue}
                onValueChange={(value) => {
                  markTouched()
                  setEffortValue(value)
                }}
                disabled={ultracode}
              >
                <SelectTrigger id="start-coding-effort" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={CLI_DEFAULT_EFFORT}>
                    CLI default
                  </SelectItem>
                  {contract.codingEffort.values.map((value) => (
                    <SelectItem key={value} value={value}>
                      {effortLabel(value)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
            <div className="space-y-0.5 pr-3">
              <Label htmlFor="start-coding-ultracode">Ultracode</Label>
              <p className="text-xs text-muted-foreground">
                Dynamic multi-agent workflows — overrides the effort level.
              </p>
            </div>
            <Switch
              id="start-coding-ultracode"
              checked={ultracode}
              onCheckedChange={(value) => {
                markTouched()
                setUltracode(value)
              }}
            />
          </div>
          <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
            <div className="space-y-0.5 pr-3">
              <Label htmlFor="start-coding-plan-mode">Plan mode</Label>
              <p className="text-xs text-muted-foreground">
                Starts with a plan you approve — from this page or at the
                desktop.
              </p>
            </div>
            <Switch
              id="start-coding-plan-mode"
              checked={planMode}
              onCheckedChange={(value) => {
                markTouched()
                setPlanMode(value)
              }}
            />
          </div>
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
            disabled={starting || !device || count === 0 || blocked}
          >
            {starting ? <Loader2 className="animate-spin" /> : <MonitorUp />}
            {isBatch ? `Start batch (${count} issues)` : `Start coding`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
