import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Link } from "@tanstack/react-router"
import { toast } from "sonner"
import type { Team } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  useTeamBoards,
  useTeamLabels,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
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
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Checkbox,
  ColorPicker,
  GlassGroup,
  GlassRow,
  GlassSectionHeader,
  Input,
  Label,
  Pill,
  Progress,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  STATUS_COLORS,
  conceptIcon,
} from "@exp/ui"

type ImportJob = Awaited<ReturnType<typeof trpc.imports.get.query>>

const POLL_MS = 1_500

const PHASE_LABELS: Record<string, string> = {
  discovering: `Reading the workspace`,
  users: `Resolving members`,
  boards: `Creating boards`,
  statuses: `Creating statuses`,
  labels: `Creating labels`,
  issues: `Importing issues`,
  links: `Linking duplicates and relations`,
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

  const refreshJob = useCallback(async () => {
    if (!activeJobId) {
      setJob(null)
      return
    }
    try {
      setJob(await trpc.imports.get.query({ jobId: activeJobId }))
    } catch (err) {
      setLoadError(trpcErrorMessage(err, `Could not load the import`))
    }
  }, [activeJobId])

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

  const leave = useCallback(() => {
    setActiveJobId(null)
    setJob(null)
    void refreshList()
  }, [refreshList])

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-semibold">Import</h2>
        <p className="text-sm text-muted-foreground">
          Bring another tracker's history into this team. Issues keep their
          identifiers, dates, priorities, assignees and comments; images are
          rehosted here.
        </p>
      </div>

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
}: {
  team: Team
  jobs: ImportJob[]
  onOpen: (jobId: string) => void
  onStarted: (jobId: string) => void
}) {
  const [apiKey, setApiKey] = useState(``)
  const [busy, setBusy] = useState(false)

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
              Paste a personal API key from Linear (Settings → Security &amp;
              access → Personal API keys). It is used to read the workspace
              and download its files, kept only while the import runs, and
              wiped after.
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
        <section className="space-y-3">
          <GlassSectionHeader label="Recent imports" count={jobs.length} />
          <GlassGroup>
            {jobs.map((row) => (
              <GlassRow key={row.id} className="justify-between">
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
                <Button type="button" variant="outline" size="sm" onClick={() => onOpen(row.id)}>
                  {IMPORT_TERMINAL_STATUSES.has(row.status) ? `View` : `Open`}
                </Button>
              </GlassRow>
            ))}
          </GlassGroup>
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
          {done > 0 ? ` ${done.toLocaleString()} so far.` : ``} This page updates by
          itself.
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
  const checkedPlanRef = useRef<string | null>(null)

  const boards = useTeamBoards(team.id)
  const labels = useTeamLabels(team.id)
  const { users } = useTeamUsers(team.id)
  const statuses = useTeamStatuses(team.id)

  useEffect(() => {
    if (!plan && job.plan) setPlan(job.plan)
  }, [job.plan, plan])

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
  const setStatus = (key: string, entry: StatusPlan) =>
    update({ statuses: { ...plan.statuses, [key]: entry } })
  const setLabel = (key: string, entry: LabelPlan) =>
    update({ labels: { ...plan.labels, [key]: entry } })
  const setUser = (key: string, entry: UserPlan) =>
    update({ users: { ...plan.users, [key]: entry } })

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

  const builtinOptions = statuses.options.filter((option) => option.builtinKey)
  const customOptions = statuses.options.filter((option) => !option.builtinKey && !option.id.startsWith(`builtin:`))

  return (
    <div className="space-y-6">
      {job.status === `failed` && (
        <Alert variant="destructive">
          <AlertTitle>The last run failed</AlertTitle>
          <AlertDescription>
            {job.error ?? `Unknown error`}. Check the plan and start again: what
            was already imported is kept and skipped.
          </AlertDescription>
        </Alert>
      )}

      <PreviewSummary preview={preview} />

      <section className="space-y-3">
        <GlassSectionHeader label="Boards" count={boardTargets.length} />
        <GlassGroup>
          {preview.supportsProjectRouting && (
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
                  ? `One board per Linear team; a project becomes a label.`
                  : `One board per project; issues without a project land on their team's board.`}
              </p>
            </div>
          )}
          {boardTargets.map((target) => {
            const entry: BoardPlan = plan.boards[target.key] ?? { mode: `skip` }
            const value =
              entry.mode === `existing` ? `existing:${entry.boardId}` : entry.mode
            return (
              <div key={target.key} className="space-y-2 p-4">
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
              </div>
            )
          })}
        </GlassGroup>
      </section>

      <section className="space-y-3">
        <GlassSectionHeader label="Statuses" count={preview.statuses.length} />
        <GlassGroup>
          {preview.statuses.map((status) => {
            const entry: StatusPlan = plan.statuses[status.key] ?? {
              mode: `create`,
              name: status.name,
              color: status.color,
              category: status.category,
            }
            const value =
              entry.mode === `builtin`
                ? `builtin:${entry.builtinKey}`
                : entry.mode === `existing`
                  ? `existing:${entry.statusId}`
                  : `create`
            const teamName = preview.teams.find((team) => team.key === status.teamKey)?.name
            return (
              <div key={status.key} className="space-y-2 p-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <span
                      className="size-3 shrink-0 rounded-full"
                      style={{ backgroundColor: status.color }}
                    />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">{status.name}</div>
                      <div className="text-xs text-muted-foreground">
                        {CATEGORY_LABELS[status.category]} · {status.issueCount.toLocaleString()} issues
                        {teamName ? ` · ${teamName}` : ``}
                      </div>
                    </div>
                  </div>
                  <Select
                    value={value}
                    onValueChange={(next) => {
                      if (next === `create`) {
                        setStatus(status.key, {
                          mode: `create`,
                          name: status.name,
                          color: status.color,
                          category: status.category,
                        })
                      } else if (next.startsWith(`builtin:`)) {
                        setStatus(status.key, {
                          mode: `builtin`,
                          builtinKey: next.replace(`builtin:`, ``) as StatusPlan extends {
                            builtinKey: infer K
                          }
                            ? K
                            : never,
                        })
                      } else {
                        setStatus(status.key, {
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
                      {status.category !== `duplicate` && (
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
                      onChange={(color) => setStatus(status.key, { ...entry, color })}
                    />
                    <Input
                      aria-label="Status name"
                      value={entry.name}
                      onChange={(event) =>
                        setStatus(status.key, { ...entry, name: event.target.value })
                      }
                    />
                  </div>
                )}
              </div>
            )
          })}
        </GlassGroup>
      </section>

      <section className="space-y-3">
        <GlassSectionHeader label="Labels" count={preview.labels.length} />
        <GlassGroup>
          {preview.labels.map((label) => {
            const entry: LabelPlan = plan.labels[label.key] ?? { mode: `create` }
            const value = entry.mode === `existing` ? `existing:${entry.labelId}` : entry.mode
            return (
              <GlassRow key={label.key} className="justify-between">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className="size-3 shrink-0 rounded-full"
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
              </GlassRow>
            )
          })}
        </GlassGroup>
      </section>

      <section className="space-y-3">
        <GlassSectionHeader label="Members" count={preview.users.length} />
        <GlassGroup>
          {preview.users.map((user) => {
            const entry: UserPlan = plan.users[user.key] ?? { mode: `self` }
            const value = entry.mode === `member` ? `member:${entry.userId}` : entry.mode
            return (
              <GlassRow key={user.key} className="justify-between">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{user.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {user.email ?? `no email`} · {user.issueCount.toLocaleString()} assigned ·{` `}
                    {user.commentCount.toLocaleString()} comments
                  </div>
                </div>
                <Select
                  value={value}
                  onValueChange={(next) => {
                    if (next === `self` || next === `invite`) setUser(user.key, { mode: next })
                    else setUser(user.key, { mode: `member`, userId: next.replace(`member:`, ``) })
                  }}
                >
                  <SelectTrigger className="w-56">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {users.map((row) => (
                      <SelectItem key={row.id} value={`member:${row.id}`}>
                        {row.id === userId ? `${row.name} (you)` : `${row.name} · ${row.email}`}
                      </SelectItem>
                    ))}
                    {user.email && <SelectItem value="invite">Invite {user.email}</SelectItem>}
                    <SelectItem value="self">Attribute to me</SelectItem>
                  </SelectContent>
                </Select>
              </GlassRow>
            )
          })}
        </GlassGroup>
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
                Status, assignee, priority and label changes appear in each issue's
                timeline ({preview.counts.events.toLocaleString()} events).
              </span>
            </span>
          </label>
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

function PreviewSummary({ preview }: { preview: ImportPreview }) {
  const stats = [
    [`Issues`, preview.counts.issues],
    [`Comments`, preview.counts.comments],
    [`Files`, preview.counts.assets],
    [`Statuses`, preview.statuses.length],
    [`Labels`, preview.labels.length],
    [`Members`, preview.users.length],
  ] as const
  return (
    <section className="space-y-3">
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
      <GlassGroup>
        <div className="grid grid-cols-3 gap-3 p-4 sm:grid-cols-6">
          {stats.map(([label, value]) => (
            <div key={label}>
              <div className="text-lg font-semibold tabular-nums">{value.toLocaleString()}</div>
              <div className="text-xs text-muted-foreground">{label}</div>
            </div>
          ))}
        </div>
        {preview.counts.assetBytes > 0 && (
          <div className="px-4 pb-3 text-xs text-muted-foreground">
            About {formatBytes(preview.counts.assetBytes)} of files will be rehosted.
          </div>
        )}
        {preview.warnings.length > 0 && (
          <ul className="space-y-1 px-4 pb-4 text-xs text-muted-foreground">
            {preview.warnings.map((warning) => (
              <li key={warning}>• {warning}</li>
            ))}
          </ul>
        )}
      </GlassGroup>
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
          You can leave this page; the import continues on the server and this
          page updates by itself.
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
        <section className="space-y-3">
          <GlassSectionHeader label="Import completed" trailing={<Pill className="text-green-500">Done</Pill>} />
          <GlassGroup>
            <div className="grid grid-cols-3 gap-3 p-4 sm:grid-cols-6">
              {(
                [
                  [`Issues`, counts.issues],
                  [`Comments`, counts.comments],
                  [`Files`, counts.attachments],
                  [`Boards`, counts.boards],
                  [`Statuses`, counts.statuses],
                  [`Labels`, counts.labels],
                ] as const
              ).map(([label, value]) => (
                <div key={label}>
                  <div className="text-lg font-semibold tabular-nums">{value.toLocaleString()}</div>
                  <div className="text-xs text-muted-foreground">{label}</div>
                </div>
              ))}
            </div>
            {(counts.relations > 0 || counts.events > 0 || counts.invites > 0) && (
              <div className="px-4 pb-3 text-xs text-muted-foreground">
                {counts.relations.toLocaleString()} relations · {counts.events.toLocaleString()} history
                events · {counts.invites} invites
              </div>
            )}
            {warnings.length > 0 && (
              <ul className="space-y-1 px-4 pb-4 text-xs text-muted-foreground">
                {warnings.map((warning) => (
                  <li key={warning}>• {warning}</li>
                ))}
              </ul>
            )}
          </GlassGroup>
        </section>
      ) : job.status === `cancelled` ? (
        <Alert>
          <AlertTitle>Import cancelled</AlertTitle>
          <AlertDescription>
            Anything imported before the stop is kept. Connect again to resume:
            the same issues are never imported twice.
          </AlertDescription>
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
