import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Link } from "@tanstack/react-router"
import { toast } from "sonner"
import type { Team } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import {
  useTeamBoards,
  useTeamLabels,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import {
  BOARD_PREFIX_PATTERN,
  IMPORT_TERMINAL_STATUSES,
  type BoardPlan,
  type DryRunResult,
  type ImportPlan,
  type ImportPreview,
  type LabelPlan,
  type StatusPlan,
  type UserPlan,
} from "@/lib/import/bundle"
import { plannedInviteEmails } from "@/lib/import/plan"
import {
  groupPreviewStatuses,
  visibleBoardKeys,
  visiblePreviewLabels,
  visiblePreviewUsers,
} from "@/lib/import/preview-view"
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Checkbox,
  ColorPicker,
  GlassGroup,
  GlassSectionHeader,
  Input,
  Label,
  ListRow,
  Pill,
  Progress,
  SETTINGS_LIST_CLASS,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  STATUS_COLORS,
  StatusGlyph,
  categoryStatusIcon,
  conceptIcon,
} from "@exp/ui"

type ImportJob = Awaited<ReturnType<typeof trpc.imports.get.query>>

const POLL_MS = 1_500

// EXP-1076: the three ways a source person reaches the roster. `skip` is
// gone — an unmatched person becomes a PLACEHOLDER member (seated now, no
// mail, no seat) so their issues and comments carry their own name.
type MemberChoice = `member` | `placeholder` | `invite`

// The Members step's seat count on the cloud: free seats minus the invites
// planned so far; below zero it names what is missing and offers seats.
function SeatCapsule({ left, onAddSeats }: { left: number; onAddSeats: () => void }) {
  if (left >= 0) {
    return (
      <Pill className="text-muted-foreground">
        {left} {left === 1 ? `seat` : `seats`} left
      </Pill>
    )
  }
  const missing = -left
  return (
    <span className="flex items-center gap-2">
      <Pill className="text-destructive">
        Needs {missing} more {missing === 1 ? `seat` : `seats`}
      </Pill>
      <Button type="button" variant="link" size="sm" className="h-auto p-0" onClick={onAddSeats}>
        Add seats
      </Button>
    </span>
  )
}

const PHASE_LABELS: Record<string, string> = {
  discovering: `Reading the workspace`,
  users: `Resolving members`,
  boards: `Creating boards`,
  statuses: `Creating statuses`,
  labels: `Creating labels`,
  issues: `Importing issues`,
  links: `Linking duplicates and relations`,
  archiving: `Archiving`,
  done: `Done`,
}

const STATUS_LABELS: Record<string, string> = {
  draft: `Ready to map`,
  previewing: `Reading workspace`,
  ready: `Queued`,
  running: `Importing`,
  completed: `Completed`,
  failed: `Failed`,
  cancelled: `Cancelled`,
}

const CATEGORY_LABELS: Record<string, string> = {
  backlog: `Backlog`,
  unstarted: `Unstarted`,
  started: `Started`,
  completed: `Completed`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

function formatDate(value: Date | string) {
  return new Date(value).toLocaleString(undefined, {
    year: `numeric`,
    month: `short`,
    day: `numeric`,
    hour: `2-digit`,
    minute: `2-digit`,
  })
}

function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

const DownloadIcon = conceptIcon(`settings-import`)
const DeleteIcon = conceptIcon(`ui-delete`)

/** The wizard's ONE summary line: a muted `a · b · c` under a section band,
 *  the same shape the preview, the dry run and the finished card all read as
 *  (EXP-1076 — no tile grids, no card in a card). */
function SummaryLine({ parts }: { parts: string[] }) {
  return (
    <p className="px-3 text-xs text-muted-foreground">{parts.join(` · `)}</p>
  )
}

function WarningList({ warnings }: { warnings: readonly string[] }) {
  if (warnings.length === 0) return null
  return (
    <ul className="mt-1 space-y-1 px-3 text-xs text-muted-foreground">
      {warnings.map((warning) => (
        <li key={warning}>• {warning}</li>
      ))}
    </ul>
  )
}

/**
 * EXP-630 tracker-import wizard (web-only, owner-only, like Billing). One
 * component whose step is the job's status: Connect → (previewing) →
 * Map + Dry run (`draft`) → Run (`ready`/`running`) → a terminal card. Reads
 * the team's boards/statuses/labels/members from the live collections and
 * everything about the job from `imports.get`, polled while it moves.
 */
export function TeamImportSection({
  team,
  teamSlug,
  userId,
}: {
  team: Team
  teamSlug: string
  userId: string
}) {
  const [jobs, setJobs] = useState<ImportJob[] | null>(null)
  const [activeJobId, setActiveJobId] = useState<string | null>(null)
  const [job, setJob] = useState<ImportJob | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  const refreshList = useCallback(async () => {
    try {
      const rows = await trpc.imports.list.query({ teamId: team.id })
      setJobs(rows)
      // Land on a live job automatically so a reload resumes the wizard.
      if (!activeJobId) {
        const live = rows.find((row) => !IMPORT_TERMINAL_STATUSES.has(row.status))
        if (live) setActiveJobId(live.id)
      }
    } catch (err) {
      setLoadError(trpcErrorMessage(err, `Could not load imports`))
    }
  }, [team.id, activeJobId])

  useEffect(() => {
    void refreshList()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [team.id])

  const leave = useCallback(() => {
    setActiveJobId(null)
    setJob(null)
    void refreshList()
  }, [refreshList])

  const refreshJob = useCallback(async () => {
    if (!activeJobId) {
      setJob(null)
      return
    }
    try {
      setJob(await trpc.imports.get.query({ jobId: activeJobId }))
    } catch (err) {
      // EXP-1076: discarding a draft DELETES the row, so the poll that was
      // already in flight 404s. That is the discard landing, not a failure —
      // fall back to the list instead of painting an error.
      if (trpcErrorCode(err) === `NOT_FOUND`) {
        leave()
        return
      }
      setLoadError(trpcErrorMessage(err, `Could not load the import`))
    }
  }, [activeJobId, leave])

  useEffect(() => {
    void refreshJob()
  }, [refreshJob])

  // Poll while the worker owns the job.
  const moving =
    job !== null &&
    (job.status === `previewing` || job.status === `ready` || job.status === `running`)
  useEffect(() => {
    if (!moving) return
    const timer = setInterval(() => {
      void refreshJob()
    }, POLL_MS)
    return () => clearInterval(timer)
  }, [moving, refreshJob])

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">Import</h2>

      {loadError && (
        <Alert variant="destructive">
          <AlertTitle>Something went wrong</AlertTitle>
          <AlertDescription>{loadError}</AlertDescription>
        </Alert>
      )}

      {job === null ? (
        <ConnectStep
          team={team}
          jobs={jobs ?? []}
          onOpen={(id) => setActiveJobId(id)}
          onStarted={(id) => {
            setActiveJobId(id)
            void refreshList()
          }}
          onCleared={refreshList}
        />
      ) : job.status === `previewing` ? (
        <PreviewingStep job={job} onCancelled={leave} />
      ) : job.status === `draft` || job.status === `failed` ? (
        <MapStep
          team={team}
          job={job}
          userId={userId}
          onChanged={refreshJob}
          onCancelled={leave}
        />
      ) : job.status === `ready` || job.status === `running` ? (
        <RunningStep job={job} onCancelled={refreshJob} />
      ) : (
        <FinishedStep job={job} teamSlug={teamSlug} onLeave={leave} />
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

function ConnectStep({
  team,
  jobs,
  onOpen,
  onStarted,
  onCleared,
}: {
  team: Team
  jobs: ImportJob[]
  onOpen: (jobId: string) => void
  onStarted: (jobId: string) => void
  onCleared: () => Promise<void> | void
}) {
  const [apiKey, setApiKey] = useState(``)
  const [busy, setBusy] = useState(false)
  const [clearing, setClearing] = useState(false)

  const clear = async () => {
    setClearing(true)
    try {
      await trpc.imports.clearHistory.mutate(
        { teamId: team.id },
        { context: { skipErrorToast: true } }
      )
      await onCleared()
    } catch (err) {
      toast.error(`Could not clear the history`, {
        description: trpcErrorMessage(err, ``),
      })
    } finally {
      setClearing(false)
    }
  }

  const connect = async () => {
    if (!apiKey.trim()) return
    setBusy(true)
    try {
      const result = await trpc.imports.connect.mutate(
        { teamId: team.id, source: `linear`, apiKey: apiKey.trim() },
        { context: { skipErrorToast: true } }
      )
      toast.success(`Connected as ${result.connectedAs}`)
      setApiKey(``)
      onStarted(result.jobId)
    } catch (err) {
      toast.error(`Could not connect to Linear`, {
        description: trpcErrorMessage(err, `Check the key and try again.`),
      })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <GlassSectionHeader label="Linear" leading={<DownloadIcon className="size-4" />} />
        <GlassGroup>
          <div className="space-y-3 p-4">
            <p className="text-sm text-muted-foreground">
              Personal API key from Linear → Settings → Security &amp; access.
            </p>
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                type="password"
                autoComplete="off"
                placeholder="lin_api_…"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === `Enter`) void connect()
                }}
                aria-label="Linear API key"
              />
              <Button type="button" onClick={() => void connect()} disabled={busy || !apiKey.trim()}>
                {busy ? `Connecting…` : `Connect`}
              </Button>
            </div>
          </div>
        </GlassGroup>
      </section>

      {jobs.length > 0 && (
        <section className="group">
          <GlassSectionHeader
            label="Recent imports"
            count={jobs.length}
            trailing={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Clear import history"
                className="shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 max-md:opacity-100"
                disabled={clearing}
                onClick={() => void clear()}
              >
                <DeleteIcon />
              </Button>
            }
          />
          <div className={SETTINGS_LIST_CLASS}>
            {jobs.map((row) => (
              <ListRow key={row.id} className="justify-between px-3 py-2">
                <div className="min-w-0">
                  <div className="text-sm font-medium">
                    {row.preview?.workspace.name ?? row.source} ·{` `}
                    {STATUS_LABELS[row.status] ?? row.status}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {formatDate(row.createdAt)}
                    {row.counts ? ` · ${row.counts.issues.toLocaleString()} issues` : ``}
                    {row.error ? ` · ${row.error}` : ``}
                  </div>
                </div>
                <Pill mode="action" onClick={() => onOpen(row.id)}>
                  {IMPORT_TERMINAL_STATUSES.has(row.status) ? `View` : `Open`}
                </Pill>
              </ListRow>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Previewing (discovery in progress)
// ---------------------------------------------------------------------------

function CancelButton({
  job,
  label = `Cancel`,
  onCancelled,
}: {
  job: ImportJob
  label?: string
  onCancelled: () => void
}) {
  const [busy, setBusy] = useState(false)
  return (
    <Button
      type="button"
      variant="outline"
      disabled={busy}
      onClick={async () => {
        setBusy(true)
        try {
          await trpc.imports.cancel.mutate({ jobId: job.id })
          onCancelled()
        } catch (err) {
          toast.error(`Could not cancel`, { description: trpcErrorMessage(err, ``) })
        } finally {
          setBusy(false)
        }
      }}
    >
      {label}
    </Button>
  )
}

function PreviewingStep({ job, onCancelled }: { job: ImportJob; onCancelled: () => void }) {
  const done = job.progress?.done ?? 0
  return (
    <GlassGroup>
      <div className="space-y-3 p-4">
        <div className="text-sm font-medium">Reading your Linear workspace…</div>
        <p className="text-sm text-muted-foreground">
          Fetching teams, statuses, labels, members, issues and comments.
          {done > 0 ? ` ${done.toLocaleString()} so far.` : ``}
        </p>
        <Progress value={null} />
        <div className="flex justify-end">
          <CancelButton job={job} onCancelled={onCancelled} />
        </div>
      </div>
    </GlassGroup>
  )
}

// ---------------------------------------------------------------------------
// Map + dry run
// ---------------------------------------------------------------------------

function MapStep({
  team,
  job,
  userId,
  onChanged,
  onCancelled,
}: {
  team: Team
  job: ImportJob
  userId: string
  onChanged: () => Promise<void>
  onCancelled: () => void
}) {
  const preview = job.preview
  const [plan, setPlan] = useState<ImportPlan | null>(job.plan)
  const [dryRun, setDryRun] = useState<DryRunResult | null>(null)
  const [checking, setChecking] = useState(false)
  const [starting, setStarting] = useState(false)
  // Free seats on the cloud (members + pending invites against the plan);
  // null = no cap; undefined = not loaded yet. The Members step counts its
  // planned invites against it.
  const [seatsLeft, setSeatsLeft] = useState<number | null | undefined>(undefined)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{ team: string | null; teamYearly: string | null }>({
    team: null,
    teamYearly: null,
  })
  useEffect(() => {
    let cancelled = false
    void trpc.teams.inviteCapacity
      .query({ teamId: team.id })
      .then(({ remaining }) => {
        if (!cancelled) setSeatsLeft(remaining)
      })
      .catch(() => {
        if (!cancelled) setSeatsLeft(null)
      })
    void getRuntimeConfig().then((config) => {
      if (cancelled) return
      setProductIds({ team: config.creemTeamProductId, teamYearly: config.creemTeamYearlyProductId })
    })
    return () => {
      cancelled = true
    }
  }, [team.id])
  const checkedPlanRef = useRef<string | null>(null)

  const boards = useTeamBoards(team.id)
  const labels = useTeamLabels(team.id)
  const { users } = useTeamUsers(team.id)
  const statuses = useTeamStatuses(team.id)

  useEffect(() => {
    if (!plan && job.plan) setPlan(job.plan)
  }, [job.plan, plan])

  const teamMembersForPlan = useMemo(
    () => users.map((row) => ({ userId: row.id, email: row.email, name: row.name })),
    [users]
  )
  // The source team behind a preview key — the hint of a status several teams
  // share names them all (EXP-1076).
  const teamNameByKey = useMemo(
    () => new Map((preview?.teams ?? []).map((row) => [row.key, row.name])),
    [preview]
  )
  const planJson = useMemo(() => JSON.stringify(plan), [plan])
  const checked = dryRun !== null && checkedPlanRef.current === planJson

  if (!preview || !plan) {
    return (
      <Alert variant="destructive">
        <AlertTitle>This import has no preview</AlertTitle>
        <AlertDescription>
          {job.error ?? `Discovery did not finish. Cancel it and connect again.`}
        </AlertDescription>
        <div className="mt-3">
          <CancelButton job={job} label="Discard" onCancelled={onCancelled} />
        </div>
      </Alert>
    )
  }

  const update = (patch: Partial<ImportPlan>) =>
    setPlan((current) => (current ? { ...current, ...patch } : current))
  const setBoard = (key: string, entry: BoardPlan) =>
    update({ boards: { ...plan.boards, [key]: entry } })
  // EXP-1076: one decision per GROUP — Linear gives every team its own
  // "Todo", Exponential one per team, so the same entry lands on every key the
  // collapsed row stands for, in ONE update.
  const setStatusGroup = (keys: readonly string[], entry: StatusPlan) => {
    const patch: Record<string, StatusPlan> = {}
    for (const key of keys) patch[key] = entry
    update({ statuses: { ...plan.statuses, ...patch } })
  }
  const setLabel = (key: string, entry: LabelPlan) =>
    update({ labels: { ...plan.labels, [key]: entry } })
  // Functional on purpose: several member rows can settle in the same tick
  // (a stored plan's legacy `self` entries migrating), and a patch built from
  // a stale `plan.users` would drop its neighbours.
  const setUser = (key: string, entry: UserPlan) =>
    setPlan((current) =>
      current ? { ...current, users: { ...current.users, [key]: entry } } : current
    )

  const check = async () => {
    setChecking(true)
    try {
      await trpc.imports.savePlan.mutate({ jobId: job.id, plan }, { context: { skipErrorToast: true } })
      const result = await trpc.imports.dryRun.query({ jobId: job.id })
      setDryRun(result)
      checkedPlanRef.current = planJson
    } catch (err) {
      toast.error(`Could not check the plan`, { description: trpcErrorMessage(err, ``) })
    } finally {
      setChecking(false)
    }
  }

  const start = async () => {
    setStarting(true)
    try {
      await trpc.imports.start.mutate({ jobId: job.id }, { context: { skipErrorToast: true } })
      toast.success(`Import started`)
      await onChanged()
    } catch (err) {
      toast.error(`Could not start the import`, { description: trpcErrorMessage(err, ``) })
    } finally {
      setStarting(false)
    }
  }

  // Which board keys the current routing routes to.
  const boardTargets =
    plan.routing === `project`
      ? [
          ...preview.projects.map((project) => ({
            key: project.key,
            name: project.name,
            hint: `${project.issueCount.toLocaleString()} issues · project`,
          })),
          ...preview.teams.map((team) => ({
            key: team.key,
            name: team.name,
            hint: `${team.issueCount.toLocaleString()} issues · fallback for issues without a project`,
          })),
        ]
      : preview.teams.map((team) => ({
          key: team.key,
          name: team.name,
          hint: `${team.issueCount.toLocaleString()} issues · ${team.prefix}`,
        }))
  // EXP-500 archive boards for the source's archived issues, only while the
  // option is on (the plan keeps their entries either way).
  const archivedIssueCount = preview.archives.reduce((sum, archive) => sum + archive.issueCount, 0)
  if (plan.importArchived) {
    for (const archive of preview.archives) {
      const team = preview.teams.find((row) => row.key === archive.teamKey)
      boardTargets.push({
        key: archive.key,
        name: archive.name,
        hint: `${archive.issueCount.toLocaleString()} archived issues${team ? ` · ${team.name}` : ``} · archived after the import`,
      })
    }
  }

  // EXP-1076: only what the plan's non-skipped boards actually carry is worth
  // mapping — a skipped Linear team must not make the operator map its
  // statuses, labels and people.
  const visible = visibleBoardKeys(preview, plan)
  const statusGroups = groupPreviewStatuses(preview.statuses, visible, teamNameByKey)
  const visibleLabels = visiblePreviewLabels(preview.labels, visible)
  const visibleUsers = visiblePreviewUsers(preview.users, visible)

  const builtinOptions = statuses.options.filter((option) => option.builtinKey)
  const customOptions = statuses.options.filter((option) => !option.builtinKey && !option.id.startsWith(`builtin:`))

  return (
    <div className="space-y-6">
      {job.status === `failed` && (
        <Alert variant="destructive">
          <AlertTitle>The last run failed</AlertTitle>
          <AlertDescription>{job.error ?? `Unknown error`}</AlertDescription>
        </Alert>
      )}

      <PreviewSummary preview={preview} />

      <section>
        <GlassSectionHeader label="Boards" count={boardTargets.length} />
        {preview.supportsProjectRouting && (
          <GlassGroup className="mb-3">
            <div className="space-y-2 p-4">
              <Label className="text-xs text-muted-foreground">Route issues by</Label>
              <SegmentedControl
                value={plan.routing}
                onValueChange={(routing) => update({ routing })}
                options={[
                  { value: `team`, label: `Linear team → board` },
                  { value: `project`, label: `Linear project → board` },
                ]}
                fill
              />
              <p className="text-xs text-muted-foreground">
                {plan.routing === `team`
                  ? `A project becomes a label.`
                  : `Projectless issues land on their team's board.`}
              </p>
            </div>
          </GlassGroup>
        )}
        <div className={SETTINGS_LIST_CLASS}>
          {boardTargets.map((target) => {
            const entry: BoardPlan = plan.boards[target.key] ?? { mode: `skip` }
            const value =
              entry.mode === `existing` ? `existing:${entry.boardId}` : entry.mode
            return (
              <ListRow
                key={target.key}
                className="flex-col items-stretch gap-2 px-3 py-2"
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{target.name}</div>
                    <div className="text-xs text-muted-foreground">{target.hint}</div>
                  </div>
                  <Select
                    value={value}
                    onValueChange={(next) => {
                      if (next === `skip`) setBoard(target.key, { mode: `skip` })
                      else if (next === `create`) {
                        setBoard(target.key, {
                          mode: `create`,
                          name: entry.mode === `create` ? entry.name : target.name,
                          prefix: entry.mode === `create` ? entry.prefix : ``,
                          numbering: `preserve`,
                        })
                      } else {
                        const boardId = next.replace(`existing:`, ``)
                        setBoard(target.key, {
                          mode: `existing`,
                          boardId,
                          numbering: entry.mode === `existing` ? entry.numbering : `allocate`,
                        })
                      }
                    }}
                  >
                    <SelectTrigger className="w-56">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="create">Create a new board</SelectItem>
                      {boards.map((board) => (
                        <SelectItem key={board.id} value={`existing:${board.id}`}>
                          {board.name} ({board.prefix})
                        </SelectItem>
                      ))}
                      {/* An archived board (an earlier run's archive) never
                          syncs, so the pick the plan made needs its own row. */}
                      {entry.mode === `existing` &&
                        !boards.some((board) => board.id === entry.boardId) && (
                          <SelectItem value={`existing:${entry.boardId}`}>
                            Archived board from an earlier import
                          </SelectItem>
                        )}
                      <SelectItem value="skip">Skip these issues</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                {entry.mode === `create` && (
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      aria-label="Board name"
                      value={entry.name}
                      onChange={(event) =>
                        setBoard(target.key, { ...entry, name: event.target.value })
                      }
                    />
                    <Input
                      aria-label="Prefix"
                      className="sm:w-28"
                      placeholder="PREFIX"
                      value={entry.prefix}
                      maxLength={4}
                      aria-invalid={!BOARD_PREFIX_PATTERN.test(entry.prefix)}
                      onChange={(event) =>
                        setBoard(target.key, {
                          ...entry,
                          prefix: event.target.value.toUpperCase(),
                        })
                      }
                    />
                  </div>
                )}
                {entry.mode !== `skip` && (
                  <label className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Checkbox
                      checked={entry.numbering === `preserve`}
                      onCheckedChange={(checked) =>
                        setBoard(target.key, {
                          ...entry,
                          numbering: checked ? `preserve` : `allocate`,
                        })
                      }
                    />
                    Keep the original issue numbers
                  </label>
                )}
              </ListRow>
            )
          })}
        </div>
      </section>

      <section>
        <GlassSectionHeader label="Statuses" count={statusGroups.length} />
        <div className={SETTINGS_LIST_CLASS}>
          {statusGroups.map((group) => {
            // The group speaks with one voice: the first key carries the
            // decision, every key receives it.
            const entry: StatusPlan = plan.statuses[group.keys[0]] ?? {
              mode: `create`,
              name: group.name,
              color: group.color,
              category: group.category,
            }
            const value =
              entry.mode === `builtin`
                ? `builtin:${entry.builtinKey}`
                : entry.mode === `existing`
                  ? `existing:${entry.statusId}`
                  : `create`
            return (
              <ListRow
                key={group.id}
                className="flex-col items-stretch gap-2 px-3 py-2"
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <StatusGlyph
                      icon={categoryStatusIcon(group.category, 0, 1)}
                      colorHex={group.color}
                      className="size-4 shrink-0"
                    />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">{group.name}</div>
                      <div className="text-xs text-muted-foreground">
                        {CATEGORY_LABELS[group.category]} ·{` `}
                        {group.issueCount.toLocaleString()} issues
                        {group.teamNames.length > 0 ? ` · ${group.teamNames.join(`, `)}` : ``}
                      </div>
                    </div>
                  </div>
                  <Select
                    value={value}
                    onValueChange={(next) => {
                      if (next === `create`) {
                        setStatusGroup(group.keys, {
                          mode: `create`,
                          name: group.name,
                          color: group.color,
                          category: group.category,
                        })
                      } else if (next.startsWith(`builtin:`)) {
                        setStatusGroup(group.keys, {
                          mode: `builtin`,
                          builtinKey: next.replace(`builtin:`, ``) as StatusPlan extends {
                            builtinKey: infer K
                          }
                            ? K
                            : never,
                        })
                      } else {
                        setStatusGroup(group.keys, {
                          mode: `existing`,
                          statusId: next.replace(`existing:`, ``),
                        })
                      }
                    }}
                  >
                    <SelectTrigger className="w-56">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {builtinOptions.map((option) => (
                        <SelectItem key={option.id} value={`builtin:${option.builtinKey}`}>
                          {option.name}
                        </SelectItem>
                      ))}
                      {customOptions.map((option) => (
                        <SelectItem key={option.id} value={`existing:${option.id}`}>
                          {option.name}
                        </SelectItem>
                      ))}
                      {group.category !== `duplicate` && (
                        <SelectItem value="create">Create as new status</SelectItem>
                      )}
                    </SelectContent>
                  </Select>
                </div>
                {entry.mode === `create` && (
                  <div className="flex items-center gap-2">
                    <ColorPicker
                      value={entry.color}
                      colors={STATUS_COLORS}
                      onChange={(color) => setStatusGroup(group.keys, { ...entry, color })}
                    />
                    <Input
                      aria-label="Status name"
                      value={entry.name}
                      onChange={(event) =>
                        setStatusGroup(group.keys, { ...entry, name: event.target.value })
                      }
                    />
                  </div>
                )}
              </ListRow>
            )
          })}
        </div>
      </section>

      <section>
        <GlassSectionHeader label="Labels" count={visibleLabels.length} />
        <div className={SETTINGS_LIST_CLASS}>
          {visibleLabels.map((label) => {
            const entry: LabelPlan = plan.labels[label.key] ?? { mode: `create` }
            const value = entry.mode === `existing` ? `existing:${entry.labelId}` : entry.mode
            return (
              <ListRow key={label.key} className="justify-between px-3 py-2">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className="h-4 w-4 shrink-0 rounded-full ring-1 ring-border"
                    style={{ backgroundColor: label.color }}
                  />
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium">{label.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {label.issueCount.toLocaleString()} issues
                    </div>
                  </div>
                </div>
                <Select
                  value={value}
                  onValueChange={(next) => {
                    if (next === `create` || next === `skip`) setLabel(label.key, { mode: next })
                    else setLabel(label.key, { mode: `existing`, labelId: next.replace(`existing:`, ``) })
                  }}
                >
                  <SelectTrigger className="w-56">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="create">Create label</SelectItem>
                    {labels.map((row) => (
                      <SelectItem key={row.id} value={`existing:${row.id}`}>
                        {row.name}
                      </SelectItem>
                    ))}
                    <SelectItem value="skip">Skip</SelectItem>
                  </SelectContent>
                </Select>
              </ListRow>
            )
          })}
        </div>
      </section>

      <section>
        <GlassSectionHeader
          label="Members"
          count={visibleUsers.length}
          trailing={
            seatsLeft !== undefined && seatsLeft !== null ? (
              <SeatCapsule
                left={seatsLeft - plannedInviteEmails(plan, teamMembersForPlan).length}
                onAddSeats={() => setUpgradeOpen(true)}
              />
            ) : undefined
          }
        />
        <div className={SETTINGS_LIST_CLASS}>
          {visibleUsers.map((user) => (
            <MemberRow
              key={user.key}
              user={user}
              entry={plan.users[user.key]}
              roster={users}
              importerId={userId}
              onChange={(next) => setUser(user.key, next)}
            />
          ))}
        </div>
        <p className="mt-2 px-3 text-xs text-muted-foreground">
          Placeholders join the roster now; invite them later from Members.
        </p>
        <UpgradeDialog
          open={upgradeOpen}
          onOpenChange={setUpgradeOpen}
          title="Out of seats"
          description="Add seats to invite everyone in this import."
          teamProductId={productIds.team}
          teamYearlyProductId={productIds.teamYearly}
          teamId={team.id}
        />
      </section>

      <section className="space-y-3">
        <GlassSectionHeader label="Options" />
        <GlassGroup>
          <label className="flex items-center gap-3 p-4 text-sm">
            <Checkbox
              checked={plan.importHistory}
              onCheckedChange={(checked) => update({ importHistory: checked === true })}
            />
            <span>
              Import activity history
              <span className="block text-xs text-muted-foreground">
                {preview.counts.events.toLocaleString()} timeline events.
              </span>
            </span>
          </label>
          {preview.archives.length > 0 && (
            <label className="flex items-center gap-3 p-4 text-sm">
              <Checkbox
                checked={plan.importArchived}
                onCheckedChange={(checked) => update({ importArchived: checked === true })}
              />
              <span>
                Import archived issues
                <span className="block text-xs text-muted-foreground">
                  {archivedIssueCount.toLocaleString()} issues, on a board
                  archived after the import.
                </span>
              </span>
            </label>
          )}
        </GlassGroup>
      </section>

      {dryRun && checked && <DryRunPanel result={dryRun} />}

      <div className="flex flex-wrap items-center justify-end gap-2">
        <CancelButton job={job} label="Discard import" onCancelled={onCancelled} />
        <Button type="button" variant="outline" onClick={() => void check()} disabled={checking}>
          {checking ? `Checking…` : checked ? `Check again` : `Check plan`}
        </Button>
        <Button
          type="button"
          onClick={() => void start()}
          disabled={!checked || starting || (dryRun?.blockers.length ?? 1) > 0}
        >
          {starting ? `Starting…` : job.status === `failed` ? `Resume import` : `Start import`}
        </Button>
      </div>
    </div>
  )
}

/**
 * ONE source person, mapped. EXP-1076: the default for anyone the roster does
 * not already know is a PLACEHOLDER — seated with their own name, no mail, no
 * seat — and nobody is ever silently attributed to the importer: picking
 * "Member" without an address match leaves the roster Select unanswered and
 * writes nothing until it is.
 */
function MemberRow({
  user,
  entry,
  roster,
  importerId,
  onChange,
}: {
  user: ImportPreview[`users`][number]
  entry: UserPlan | undefined
  roster: readonly { id: string; name: string; email: string }[]
  importerId: string
  onChange: (entry: UserPlan) => void
}) {
  const email = user.email?.trim() ?? ``
  const matched = useMemo(
    () =>
      email
        ? roster.find((row) => row.email.toLowerCase() === email.toLowerCase())
        : undefined,
    [roster, email]
  )
  const [picking, setPicking] = useState(false)

  // A plan stored before EXP-1076 can still say `self`. With an address that
  // IS a placeholder now, so migrate it rather than showing one thing and
  // saving another; with no address there is nothing to seat.
  const legacySelf = entry?.mode === `self`
  useEffect(() => {
    if (legacySelf && email) onChange({ mode: `placeholder`, name: user.name, email })
  }, [legacySelf, email, user.name, onChange])

  const effective: UserPlan =
    entry ??
    (matched
      ? { mode: `member`, userId: matched.id }
      : { mode: `placeholder`, name: user.name, email })
  const named =
    effective.mode === `placeholder` || effective.mode === `invite` ? effective : null
  const choice: MemberChoice = picking
    ? `member`
    : effective.mode === `self`
      ? `placeholder`
      : effective.mode

  const setNamed = (patch: { name?: string; email?: string }) => {
    const name = patch.name ?? named?.name ?? user.name
    const address = patch.email ?? named?.email ?? email
    onChange(
      effective.mode === `invite`
        ? { mode: `invite`, name, email: address }
        : { mode: `placeholder`, name, email: address }
    )
  }

  return (
    <ListRow className="flex-col items-stretch gap-3 px-3 py-2 md:flex-row md:items-center md:justify-between">
      <div className="min-w-0">
        <div className="truncate text-sm font-medium">{user.name}</div>
        <div className="truncate text-xs text-muted-foreground">
          {user.email ?? `no email`} · {user.issueCount.toLocaleString()} assigned ·{` `}
          {user.commentCount.toLocaleString()} comments
        </div>
      </div>
      <div className="flex shrink-0 flex-col items-stretch gap-2 md:items-end">
        {effective.mode === `self` ? (
          <span className="text-xs text-muted-foreground">Attributed to you</span>
        ) : (
          <SegmentedControl<MemberChoice>
            value={choice}
            onValueChange={(next) => {
              if (next === `member`) {
                if (matched) {
                  setPicking(false)
                  onChange({ mode: `member`, userId: matched.id })
                } else {
                  setPicking(true)
                }
                return
              }
              setPicking(false)
              if (next === `placeholder`)
                onChange({ mode: `placeholder`, name: user.name, email })
              else onChange({ mode: `invite`, name: user.name, email })
            }}
            options={[
              { value: `member` as const, label: `Member` },
              { value: `placeholder` as const, label: `Placeholder` },
              ...(email ? [{ value: `invite` as const, label: `Invite` }] : []),
            ]}
          />
        )}
        {choice === `member` && (
          <Select
            // An unanswered pick keeps the trigger on its placeholder: the
            // empty value matches no item, so nothing is chosen for anyone.
            value={effective.mode === `member` ? effective.userId : ``}
            onValueChange={(next) => {
              setPicking(false)
              onChange({ mode: `member`, userId: next })
            }}
          >
            <SelectTrigger className="md:w-64">
              <SelectValue placeholder="Pick a member" />
            </SelectTrigger>
            <SelectContent>
              {roster.map((row) => (
                <SelectItem key={row.id} value={row.id}>
                  {row.id === importerId ? `${row.name} (you)` : `${row.name} · ${row.email}`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        {named && (
          <div className="flex flex-col gap-2 sm:flex-row">
            <Input
              value={named.name}
              onChange={(e) => setNamed({ name: e.target.value })}
              placeholder="Name"
              aria-label={`Name for ${user.name}`}
              className="sm:w-40"
            />
            <Input
              type="email"
              value={named.email}
              onChange={(e) => setNamed({ email: e.target.value })}
              placeholder="teammate@example.com"
              aria-label={`Email for ${user.name}`}
              className="sm:w-64"
            />
          </div>
        )}
      </div>
    </ListRow>
  )
}

function PreviewSummary({ preview }: { preview: ImportPreview }) {
  const counts = preview.counts
  return (
    <section>
      <GlassSectionHeader
        label={preview.workspace.name}
        trailing={
          preview.workspace.url ? (
            <a
              className="text-xs text-muted-foreground underline-offset-2 hover:underline"
              href={preview.workspace.url}
              target="_blank"
              rel="noreferrer"
            >
              Open in {preview.sourceLabel}
            </a>
          ) : undefined
        }
      />
      <SummaryLine
        parts={[
          `${counts.issues.toLocaleString()} issues`,
          `${counts.comments.toLocaleString()} comments`,
          `${counts.assets.toLocaleString()} files`,
          ...(counts.assetBytes > 0 ? [formatBytes(counts.assetBytes)] : []),
        ]}
      />
      <WarningList warnings={preview.warnings} />
    </section>
  )
}

function DryRunPanel({ result }: { result: DryRunResult }) {
  const counts = result.counts
  const summary = [
    `${counts.issues.toLocaleString()} issues`,
    `${counts.comments.toLocaleString()} comments`,
    `${counts.attachments.toLocaleString()} files (${formatBytes(counts.assetBytes)})`,
    `${counts.boardsToCreate} new boards`,
    `${counts.statusesToCreate} new statuses`,
    `${counts.labelsToCreate} new labels`,
    ...(counts.invites > 0 ? [`${counts.invites} invites`] : []),
    ...(counts.events > 0 ? [`${counts.events.toLocaleString()} history events`] : []),
    ...(counts.alreadyImported > 0 ? [`${counts.alreadyImported.toLocaleString()} already imported`] : []),
    ...(counts.skippedIssues > 0 ? [`${counts.skippedIssues.toLocaleString()} skipped`] : []),
  ]
  return (
    <section className="space-y-3">
      <GlassSectionHeader
        label="Dry run"
        trailing={
          result.blockers.length > 0 ? (
            <Pill className="border-destructive/40 text-destructive">{result.blockers.length} blocker(s)</Pill>
          ) : (
            <Pill className="text-green-500">Ready</Pill>
          )
        }
      />
      <GlassGroup>
        <div className="space-y-3 p-4 text-sm">
          <p className="text-muted-foreground">{summary.join(` · `)}</p>
          {result.blockers.length > 0 && (
            <Alert variant="destructive">
              <AlertTitle>Fix these before starting</AlertTitle>
              <AlertDescription>
                <ul className="list-disc space-y-1 pl-4">
                  {result.blockers.map((blocker) => (
                    <li key={blocker}>{blocker}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          )}
          {result.warnings.length > 0 && (
            <ul className="space-y-1 text-xs text-muted-foreground">
              {result.warnings.map((warning) => (
                <li key={warning}>• {warning}</li>
              ))}
            </ul>
          )}
        </div>
      </GlassGroup>
    </section>
  )
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

function RunningStep({ job, onCancelled }: { job: ImportJob; onCancelled: () => Promise<void> }) {
  const progress = job.progress
  const total = progress?.total ?? 0
  const done = progress?.done ?? 0
  const percent = total > 0 ? Math.round((done / total) * 100) : null
  return (
    <GlassGroup>
      <div className="space-y-3 p-4">
        <div className="flex items-center justify-between">
          <div className="text-sm font-medium">
            {job.status === `ready`
              ? `Waiting for the import worker…`
              : (PHASE_LABELS[progress?.phase ?? ``] ?? `Importing…`)}
          </div>
          {percent !== null && progress?.phase === `issues` && (
            <div className="text-xs tabular-nums text-muted-foreground">
              {done.toLocaleString()} / {total.toLocaleString()}
            </div>
          )}
        </div>
        <Progress value={progress?.phase === `issues` ? percent : null} />
        <p className="text-xs text-muted-foreground">
          You can leave this page; the import continues on the server.
        </p>
        {progress && progress.warnings.length > 0 && (
          <ul className="space-y-1 text-xs text-muted-foreground">
            {progress.warnings.slice(-5).map((warning) => (
              <li key={warning}>• {warning}</li>
            ))}
          </ul>
        )}
        <div className="flex justify-end">
          <CancelButton job={job} label="Stop import" onCancelled={() => void onCancelled()} />
        </div>
      </div>
    </GlassGroup>
  )
}

// ---------------------------------------------------------------------------
// Finished
// ---------------------------------------------------------------------------

function FinishedStep({
  job,
  teamSlug,
  onLeave,
}: {
  job: ImportJob
  teamSlug: string
  onLeave: () => void
}) {
  const counts = job.counts
  const warnings = job.progress?.warnings ?? []
  return (
    <div className="space-y-6">
      {job.status === `completed` && counts ? (
        <section>
          <GlassSectionHeader label="Import completed" trailing={<Pill className="text-green-500">Done</Pill>} />
          <SummaryLine
            parts={[
              `${counts.issues.toLocaleString()} issues`,
              `${counts.comments.toLocaleString()} comments`,
              `${counts.attachments.toLocaleString()} files`,
              `${counts.boards} boards`,
              `${counts.statuses} statuses`,
              `${counts.labels} labels`,
              ...(counts.members > 0 ? [`${counts.members} placeholders`] : []),
              ...(counts.invites > 0 ? [`${counts.invites} invites`] : []),
              ...(counts.relations > 0
                ? [`${counts.relations.toLocaleString()} relations`]
                : []),
              ...(counts.events > 0
                ? [`${counts.events.toLocaleString()} history events`]
                : []),
            ]}
          />
          <WarningList warnings={warnings} />
        </section>
      ) : job.status === `cancelled` ? (
        <Alert>
          <AlertTitle>Import cancelled</AlertTitle>
          <AlertDescription>Imported issues are kept.</AlertDescription>
        </Alert>
      ) : (
        <Alert variant="destructive">
          <AlertTitle>Import failed</AlertTitle>
          <AlertDescription>{job.error ?? `Unknown error`}</AlertDescription>
        </Alert>
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" variant="outline" onClick={onLeave}>
          Back to imports
        </Button>
        {job.status === `completed` && (
          <Button asChild>
            <Link to="/t/$teamSlug" params={{ teamSlug }}>
              Open the team
            </Link>
          </Button>
        )}
      </div>
    </div>
  )
}
