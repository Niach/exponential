import { useEffect, useMemo, useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import {
  AccountPicker,
  agentLabel,
  Button,
  Combobox,
  conceptIcon,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  getDeviceIcon,
  GlassGroup,
  GlassSectionHeader,
  Input,
  SegmentedControl,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  useIsMobile,
} from "@exp/ui"
import {
  WORKFLOW_MAX_PARALLEL_CAP,
  WORKFLOW_MAX_PARALLEL_DEFAULT,
  wfNodeKindValues,
  wfRiskValues,
  wfStartOnValues,
  WORKFLOW_DEFAULT_LAUNCH_BY_AGENT,
  type WfNodeKind,
  type WfRisk,
  type WfStartOn,
  type WorkflowLaunch,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import {
  availableFaces,
  faceLabel,
  type WorkFaceKind,
} from "@/lib/work-faces"
import type { Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { IssueChip } from "@/components/issue-chip"
import { RunningIndicator } from "@/components/agent-session-row"
import { WorkflowGraph } from "@/components/workflow-graph"
import {
  CLI_DEFAULT_EFFORT,
  CLI_DEFAULT_MODEL,
  effortLabel,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { relativeTime } from "@/components/comment-rows/format"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import { useWorkflowNodeRuns, useWorkflowNodes } from "@/hooks/use-workflows"
import { healthBadgeLabel, SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
import { accountOptionKey } from "@/lib/accounts/account-option"
import { accountOptionsOf } from "@/components/launch-dialog/use-launch-options"
import { BUILTIN_PLAN_WORKFLOW_ID } from "@/lib/builtin-actions"
import {
  agentEffortValues,
  agentModelValues,
  agentSupportsSubagentModel,
} from "@/lib/coding-launch-prefs"
import { workflowCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { cn } from "@/lib/utils"
import {
  workflowCycleNote,
  workflowFinalPrCaption,
  workflowMergeTrain,
  workflowMetricRows,
  workflowNodeKindLabel,
  workflowReviewLine,
  workflowShapeLine,
  workflowNodeTitle,
  RUNNING_NOW_LABEL,
  workflowStartBlocker,
  workflowTrainStepLabel,
  ADMIT_NODE_LABEL,
  AGENT_REVIEW_TITLE,
  APPROVE_NODE_LABEL,
  CANCEL_WORKFLOW_CONFIRM,
  CANCEL_WORKFLOW_LABEL,
  CONTRACT_PUBLISHED_LABEL,
  DELETE_WORKFLOW_LABEL,
  DISMISS_NODE_LABEL,
  MERGE_TRAIN_EMPTY,
  MERGE_TRAIN_TITLE,
  METRICS_TITLE,
  PAUSE_WORKFLOW_LABEL,
  PLAN_WORKFLOW_LABEL,
  PROPOSED_NODE_NOTE,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
  CONTRACT_MODEL_LABEL,
  INTEGRATION_MODEL_LABEL,
  REVIEW_MODEL_LABEL,
  RISK_MODEL_LABEL,
  SAME_AS_MODEL_LABEL,
  SKIP_NODE_CONFIRM,
  SKIP_NODE_LABEL,
  START_WORKFLOW_LABEL,
  WITHDRAW_APPROVAL_LABEL,
} from "@/lib/workflow-view"

// EXP-981: ONE workflow — its name, the shape line, the graph, the node panel
// beside it and the start configuration under it. Every write is a
// `workflows.*` mutation whose txId the synced collection echoes back, so the
// page never keeps a copy of the row.
//
// EXP-982: a draft can be STARTED. The header's actions follow the status
// (Start · Pause · Resume · Cancel workflow), the configuration turns
// read-only the moment the workflow leaves draft, the merge train under the
// graph says what lands next, and the node panel carries the two things only a
// person can do: approve a PR for the train, and unstick a failed node.
//
// EXP-983: every start rule runs, so the Start picker no longer holds a draft
// back, and the node panel says the two things a speculative start adds — the
// contract this node published, and the sibling work it merges in first.
//
// EXP-984: the run learns to judge itself and to grow. The node panel gains
// the agent reviewer's latest verdict, the two decisions a follow-up filed
// mid-run needs (Admit · Dismiss); the
// configuration gains the model reviews run on; and a started workflow
// carries its counters under the graph.

const WorkflowIcon = conceptIcon(`nav-workflows`)
const DeleteIcon = conceptIcon(`ui-delete`)
const PlanIcon = conceptIcon(`action-run`)
const StartIcon = conceptIcon(`action-run`)
const ResumeIcon = conceptIcon(`run-resume`)
const CancelIcon = conceptIcon(`ui-stop`)

/** The run configuration's labels. Byte-identical ×4 (the brief's words). */
const START_ON_LABELS: Record<string, string> = {
  contract: `On contract`,
  pr_open: `On PR open`,
  landed: `When landed`,
}
const RISK_LABELS: Record<string, string> = {
  low: `Low`,
  medium: `Medium`,
  high: `High`,
}
/** EXP-983: the node panel's serialization line. Byte-identical ×4. */
const MERGES_IN_FIRST_LABEL = `Merges in first`

/** EXP-1002: the phase rows' blank pick — the workflow's own Model, which
 *  is NOT `CLI_DEFAULT_MODEL` (that one means the device's default). */
const SAME_AS_MODEL = `same-as-model`

/** The four status-only mutations, and what to say when one is refused. */
type WorkflowIntent = `start` | `pause` | `resume` | `cancel`
const INTENT_ERROR: Record<WorkflowIntent, string> = {
  start: `The workflow could not be started`,
  pause: `The workflow could not be paused`,
  resume: `The workflow could not be resumed`,
  cancel: `The workflow could not be cancelled`,
}

/** What one `workflows.update` call may carry. */
interface WorkflowPatch {
  name?: string
  deviceId?: string | null
  launch?: WorkflowLaunch
  startOn?: WfStartOn
}

export function WorkflowDetail({
  workflow,
  teamSlug,
}: {
  workflow: SyncedWorkflow
  teamSlug: string
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const nodes = useWorkflowNodes(workflow.id)
  const { relations, issues } = useTeamIssueGraph(workflow.teamId)
  const issueById = useMemo(
    () => new Map(issues.map((issue) => [issue.id, issue])),
    [issues]
  )
  const runByNodeId = useWorkflowNodeRuns(workflow.teamId, nodes, issueById)
  // The runs that are up right now, one tap away: a node's circle says THAT
  // it runs, this strip is the way in. `nodes` is already in (wave, lane).
  const liveRuns = nodes.flatMap((node) => {
    const run = runByNodeId.get(node.id)
    return run?.live ? [{ node, run }] : []
  })
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [cancelOpen, setCancelOpen] = useState(false)
  const openComposer = useOpenComposer()

  // A node that left the graph (a replan folded it into a compound one) must
  // not keep a panel open on a row nobody can see any more.
  const selectedNode =
    nodes.find((node) => node.id === selectedNodeId) ?? null
  useEffect(() => {
    if (selectedNodeId && !selectedNode) setSelectedNodeId(null)
  }, [selectedNodeId, selectedNode])

  const save = async (patch: WorkflowPatch) => {
    setError(null)
    try {
      const { txId } = await trpc.workflows.update.mutate(
        { id: workflow.id, ...patch },
        { context: { skipErrorToast: true } }
      )
      await workflowCollection.utils.awaitTxId(txId)
    } catch (caught) {
      setError(trpcErrorMessage(caught, `The workflow could not be updated`))
    }
  }

  // The four run intents write nothing but the workflow's status: the engine
  // on the runner device reads it off Electric and does the rest.
  const intent = async (which: WorkflowIntent) => {
    setError(null)
    const input = { id: workflow.id }
    const options = { context: { skipErrorToast: true } }
    try {
      const { txId } =
        which === `start`
          ? await trpc.workflows.start.mutate(input, options)
          : which === `pause`
            ? await trpc.workflows.pause.mutate(input, options)
            : which === `resume`
              ? await trpc.workflows.resume.mutate(input, options)
              : await trpc.workflows.cancel.mutate(input, options)
      await workflowCollection.utils.awaitTxId(txId)
    } catch (caught) {
      setError(trpcErrorMessage(caught, INTENT_ERROR[which]))
    }
  }

  const remove = async () => {
    setDeleteOpen(false)
    setError(null)
    try {
      await trpc.workflows.delete.mutate(
        { id: workflow.id },
        { context: { skipErrorToast: true } }
      )
      void navigate({ to: `/t/$teamSlug/workflows`, params: { teamSlug } })
    } catch (caught) {
      setError(trpcErrorMessage(caught, `The workflow could not be deleted`))
    }
  }

  const cycleNote = workflowCycleNote(workflow.metrics)
  const startBlocker = workflowStartBlocker(workflow, workflow.metrics)
  // A cycle already has its own line above the buttons; saying it twice only
  // makes the header longer.
  const blockerCaption = startBlocker === cycleNote ? null : startBlocker
  const panel = selectedNode ? (
    <WorkflowNodePanel
      workflowId={workflow.id}
      node={selectedNode}
      nodes={nodes}
      issueById={issueById}
      teamSlug={teamSlug}
      workflowStatus={workflow.status}
      onError={setError}
      onClose={() => setSelectedNodeId(null)}
    />
  ) : null

  return (
    <div className="flex flex-col gap-6" data-testid="workflow-detail">
      <header className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <WorkflowIcon className="size-4 shrink-0 text-muted-foreground" />
          <WorkflowNameField
            name={workflow.name}
            onRename={(name) => void save({ name })}
          />
        </div>
        <p className="px-1 text-xs text-muted-foreground">
          {workflowShapeLine(workflow.metrics)}
        </p>
        {cycleNote && (
          <p className="px-1 text-xs text-destructive" data-testid="workflow-cycle-note">
            {cycleNote}
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2 pt-1">
          {workflow.status === `draft` && (
            <Button
              data-testid="workflow-start"
              disabled={startBlocker !== null}
              onClick={() => void intent(`start`)}
            >
              <StartIcon className="size-4" />
              {START_WORKFLOW_LABEL}
            </Button>
          )}
          {workflow.status === `running` && (
            <Button
              variant="outline"
              data-testid="workflow-pause"
              onClick={() => void intent(`pause`)}
            >
              {PAUSE_WORKFLOW_LABEL}
            </Button>
          )}
          {workflow.status === `paused` && (
            <Button
              variant="outline"
              data-testid="workflow-resume"
              onClick={() => void intent(`resume`)}
            >
              <ResumeIcon className="size-4" />
              {RESUME_WORKFLOW_LABEL}
            </Button>
          )}
          {workflow.status === `draft` && (
            <Button
              variant="outline"
              data-testid="workflow-plan"
              onClick={() =>
                openComposer({
                  actionId: BUILTIN_PLAN_WORKFLOW_ID,
                  workflowId: workflow.id,
                })
              }
            >
              <PlanIcon className="size-4" />
              {PLAN_WORKFLOW_LABEL}
            </Button>
          )}
          {(workflow.status === `running` || workflow.status === `paused`) && (
            <Button
              variant="ghost"
              className="text-destructive hover:text-destructive"
              data-testid="workflow-cancel"
              onClick={() => setCancelOpen(true)}
            >
              <CancelIcon className="size-4" />
              {CANCEL_WORKFLOW_LABEL}
            </Button>
          )}
          {workflow.status !== `running` && workflow.status !== `paused` && (
            <Button
              variant="ghost"
              className="text-destructive hover:text-destructive"
              data-testid="workflow-delete"
              onClick={() => setDeleteOpen(true)}
            >
              <DeleteIcon className="size-4" />
              {DELETE_WORKFLOW_LABEL}
            </Button>
          )}
        </div>
        {workflow.status === `draft` && blockerCaption && (
          <p
            className="px-1 text-xs text-muted-foreground"
            data-testid="workflow-start-blocker"
          >
            {blockerCaption}
          </p>
        )}
      </header>

      {error && (
        <p className="text-sm text-destructive" data-testid="workflow-error">
          {error}
        </p>
      )}

      {liveRuns.length > 0 && (
        <div
          className="flex min-w-0 flex-wrap items-center gap-1.5"
          data-testid="workflow-running-strip"
        >
          <span className="text-xs text-muted-foreground">{RUNNING_NOW_LABEL}</span>
          {liveRuns.map(({ node, run }) => {
            const issue = issueById.get(node.issueId)
            return (
              <Button key={node.id} variant="outline" size="xs" asChild>
                <Link
                  to="/t/$teamSlug/sessions/$sessionId"
                  params={{ teamSlug, sessionId: run.sessionId }}
                  data-testid={`workflow-running-${node.id}`}
                >
                  <RunningIndicator state={run.state} working={run.working} />
                  {issue
                    ? workflowNodeTitle(issue.identifier, node.memberIssueIds.length)
                    : node.issueId.slice(0, 8)}
                </Link>
              </Button>
            )
          })}
        </div>
      )}

      {/* md+: the graph with the node panel beside it; phones get the panel as
          a sheet so the grid keeps the full width. */}
      <div className="flex min-w-0 gap-4">
        <WorkflowGraph
          nodes={nodes}
          relations={relations}
          issueById={issueById}
          workflowStatus={workflow.status}
          cycleEdges={workflow.metrics.cycleEdges ?? []}
          finalPr={{
            caption: workflowFinalPrCaption(
              nodes,
              // The zod select schema widens a pg enum past its four values;
              // the view helper only ever reads it as a string.
              (workflow.finalPrState as string | null) ?? null,
              workflow.finalPrNumber
            ),
            url: workflow.finalPrUrl,
          }}
          runByNodeId={runByNodeId}
          selectedNodeId={selectedNodeId}
          onSelect={setSelectedNodeId}
          className="min-w-0 flex-1"
        />
        {!isMobile && panel && (
          <aside className="w-64 shrink-0">{panel}</aside>
        )}
      </div>
      {isMobile && (
        <Sheet
          open={panel !== null}
          onOpenChange={(open) => {
            if (!open) setSelectedNodeId(null)
          }}
        >
          <SheetContent side="bottom" className="gap-0 pb-[max(1rem,env(safe-area-inset-bottom))]">
            <SheetHeader className="pb-2">
              <SheetTitle>Node</SheetTitle>
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto pb-2">{panel}</div>
          </SheetContent>
        </Sheet>
      )}

      {workflow.status !== `draft` && (
        <MergeTrainStrip
          nodes={nodes}
          issueById={issueById}
          stacked={isMobile}
        />
      )}

      {workflow.status !== `draft` && (
        <MetricsSection metrics={workflow.metrics} />
      )}

      <HowItRunsSection workflow={workflow} onSave={save} />

      <Dialog open={cancelOpen} onOpenChange={setCancelOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{CANCEL_WORKFLOW_LABEL}</DialogTitle>
            <DialogDescription>{CANCEL_WORKFLOW_CONFIRM}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel>Keep running</DialogCancel>
            <Button
              variant="destructive"
              data-testid="workflow-cancel-confirm"
              onClick={() => {
                setCancelOpen(false)
                void intent(`cancel`)
              }}
            >
              {CANCEL_WORKFLOW_LABEL}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{DELETE_WORKFLOW_LABEL}</DialogTitle>
            <DialogDescription>
              {`"${workflow.name}" and its plan are deleted. The issues themselves stay where they are.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel>Cancel</DialogCancel>
            <Button
              variant="destructive"
              data-testid="workflow-delete-confirm"
              onClick={() => void remove()}
            >
              {DELETE_WORKFLOW_LABEL}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

/** The name, edited in place: a plain field that saves on blur (the issue
 *  title's rule) and never fights an Electric echo while it has focus. */
function WorkflowNameField({
  name,
  onRename,
}: {
  name: string
  onRename: (name: string) => void
}) {
  const [draft, setDraft] = useState(name)
  const [editing, setEditing] = useState(false)
  useEffect(() => {
    if (!editing) setDraft(name)
  }, [name, editing])
  return (
    <Input
      value={draft}
      aria-label="Workflow name"
      data-testid="workflow-name"
      className="h-8 border-0 bg-transparent px-1 text-base font-semibold shadow-none focus-visible:ring-0"
      onFocus={() => setEditing(true)}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => {
        setEditing(false)
        const next = draft.trim()
        if (next && next !== name) onRename(next)
        else setDraft(name)
      }}
      onKeyDown={(event) => {
        if (event.key === `Enter`) event.currentTarget.blur()
        if (event.key === `Escape`) {
          setDraft(name)
          event.currentTarget.blur()
        }
      }}
    />
  )
}

/** EXP-982: what is waiting to land, in landing order. A node's PR merges into
 *  the workflow's integration branch, one at a time, so this strip is the
 *  queue: the first cleared node lands next, anything still wanting a person
 *  says so. Hidden on a draft (nothing is up yet). */
function MergeTrainStrip({
  nodes,
  issueById,
  stacked,
}: {
  nodes: readonly WorkflowNode[]
  issueById: ReadonlyMap<string, Issue>
  /** Phones read the train as a short list rather than a horizontal strip. */
  stacked: boolean
}) {
  const entries = workflowMergeTrain(nodes)
  const nodeById = new Map(nodes.map((node) => [node.id, node]))
  return (
    <section className="flex flex-col" data-testid="workflow-merge-train">
      <GlassSectionHeader label={MERGE_TRAIN_TITLE} />
      {entries.length === 0 ? (
        <p className="px-1 pt-1 text-xs text-muted-foreground">
          {MERGE_TRAIN_EMPTY}
        </p>
      ) : (
        <div
          className={cn(
            `flex gap-2 pt-1`,
            stacked ? `flex-col` : `overflow-x-auto`
          )}
        >
          {entries.map((entry) => {
            const node = nodeById.get(entry.id)
            const issue = node ? issueById.get(node.issueId) : undefined
            const title =
              issue && node
                ? workflowNodeTitle(issue.identifier, node.memberIssueIds.length)
                : entry.id.slice(0, 8)
            return (
              <div
                key={entry.id}
                data-testid={`workflow-train-${entry.id}`}
                className={cn(
                  `flex min-w-0 flex-col gap-0.5 rounded-md border border-glass-stroke bg-glass-row px-2 py-1`,
                  !stacked && `shrink-0`
                )}
              >
                <span className="truncate font-mono text-xs">{title}</span>
                <span className="truncate text-xs text-muted-foreground">
                  {workflowTrainStepLabel(entry.step)}
                </span>
              </div>
            )
          })}
        </div>
      )}
    </section>
  )
}

/** EXP-984: the run's counters, straight out of the synced `metrics` jsonb.
 *  Hidden on a draft: a plan that never ran has nothing to count yet. The
 *  ROWS are the view helper's call — which ones appear, and how each reads. */
function MetricsSection({ metrics }: { metrics: Record<string, unknown> }) {
  const rows = workflowMetricRows(metrics)
  return (
    <section className="flex flex-col" data-testid="workflow-metrics">
      <GlassSectionHeader label={METRICS_TITLE} />
      <GlassGroup>
        {rows.map((row) => (
          <div
            key={row.label}
            className="flex items-center gap-3 px-4 py-3"
            data-testid={`workflow-metric-${row.label}`}
          >
            <span className="shrink-0 text-sm text-foreground">{row.label}</span>
            <span className="ml-auto truncate text-sm text-foreground/70">
              {row.value}
            </span>
          </div>
        ))}
      </GlassGroup>
    </section>
  )
}

/** The launch as the web sends it: every EXP-1002 phase pin explicit, `null`
 *  when unset (the server keeps a pin whose key is absent). */
export function explicitPhasePins(launch: WorkflowLaunch): WorkflowLaunch {
  return {
    ...launch,
    contractModel: launch.contractModel ?? null,
    integrationModel: launch.integrationModel ?? null,
    riskModel: launch.riskModel ?? null,
  }
}

/** The start configuration, persisted field by field with `workflows.update`.
 *  Same vocabulary as the Agent composer's options line — device, agent,
 *  model, subagent model (claude), effort, account — plus the workflow's own
 *  parallelism, gate and start rule. */
function HowItRunsSection({
  workflow,
  onSave,
}: {
  workflow: SyncedWorkflow
  onSave: (patch: WorkflowPatch) => void | Promise<void>
}) {
  const { data: session } = useSession()
  const remote = useRemoteStart({
    currentUserId: session?.user?.id,
    teamId: workflow.teamId,
  })
  const devices = remote.devices ?? []
  const device = devices.find(
    (candidate) => candidate.deviceId === workflow.deviceId
  )
  const launch = workflow.launch ?? {}
  const agent = launch.agent || contract.codingAgent.values[0]!
  // `workflows.update` replaces the launch whole, EXCEPT the three phase pins:
  // there an ABSENT key means "keep what is stored" (older clients never send
  // them), so this sender always names all three — `null` is how one clears.
  const patchLaunch = (patch: Partial<WorkflowLaunch>) =>
    void onSave({ launch: explicitPhasePins({ ...launch, ...patch }) })

  // A different agent has a different model vocabulary, so every model pin is
  // re-seeded from THAT agent's shipped split rather than blanked (stale
  // values would only be refused by the router). Effort is cleared: it has no
  // shipped default.
  const agentSwitchPatch = (value: string): Partial<WorkflowLaunch> => ({
    model: null,
    contractModel: null,
    integrationModel: null,
    riskModel: null,
    subagentModel: null,
    ...WORKFLOW_DEFAULT_LAUNCH_BY_AGENT[value],
    agent: value,
    effort: null,
  })

  // EXP-872: ONE account row — the machine's flattened logins (both agents,
  // by email, the device default first); a pick implies the agent. The row
  // reads the stored pair back: the named profile, else the agent's default
  // login.
  const accountOptions = accountOptionsOf(device).map((option) => ({
    key: accountOptionKey(option),
    agent: option.agent,
    email: option.email,
    hint: healthBadgeLabel(option.health) ?? undefined,
    limits: option.limits,
  }))
  const pickedAccount =
    accountOptions.find(
      (option) => option.agent === agent && option.key.endsWith(`:${launch.account}`)
    ) ?? accountOptions.find((option) => option.agent === agent)

  const agents = device?.agents?.length
    ? device.agents
    : [...contract.codingAgent.values]

  // EXP-982: a started workflow's configuration is history — the engine has
  // been cutting branches off it. Every row stays readable, none of them
  // writable.
  const readOnly = workflow.status !== `draft`

  return (
    <section className="flex flex-col" data-testid="workflow-how-it-runs">
      <GlassSectionHeader label="How it runs" />
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Runner device"
          disabled={readOnly}
          value={workflow.deviceId}
          triggerLabel="Select a device"
          options={devices.map((candidate) => ({
            value: candidate.deviceId,
            label: `${candidate.deviceLabel || candidate.deviceId}${
              candidate.owner ? ` — ${candidate.owner.name}` : ``
            }`,
            icon: getDeviceIcon(candidate),
          }))}
          onChange={(value) => {
            if (value !== null) void onSave({ deviceId: value })
          }}
        />
        {accountOptions.length > 0 ? (
          <AccountPicker
            variant="row"
            mobileTitle="Account"
            disabled={readOnly}
            value={pickedAccount?.key ?? null}
            options={accountOptions}
            onChange={(key) => {
              const option = accountOptions.find((candidate) => candidate.key === key)
              if (!option) return
              const [, id] = key.split(`:`, 2)
              // A different agent has a different model/effort vocabulary —
              // stale values would only be refused by the router.
              patchLaunch({
                ...(option.agent !== agent ? agentSwitchPatch(option.agent) : {}),
                account: id && id !== SYSTEM_PROFILE_ID ? id : null,
              })
            }}
          />
        ) : (
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Agent"
            disabled={readOnly}
            value={agent}
            options={agents.map((value) => ({
              value,
              label: agentLabel(value),
            }))}
            onChange={(value) => {
              if (value !== null) patchLaunch(agentSwitchPatch(value))
            }}
          />
        )}
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Model"
          disabled={readOnly}
          value={launch.model || CLI_DEFAULT_MODEL}
          options={[
            // A workflow always offers "no model picked" — an agent that
            // cannot be launched blank (claude) still runs on ITS default, and
            // a row with no matching option would simply read empty.
            { value: CLI_DEFAULT_MODEL, label: `Default` },
            ...agentModelValues(agent).map((value) => ({
              value,
              label: modelLabel(value),
            })),
          ]}
          onChange={(value) => {
            if (value !== null) {
              patchLaunch({ model: value === CLI_DEFAULT_MODEL ? null : value })
            }
          }}
        />
        {/* EXP-1002: the pins that may opt OUT of the model above — the two
            phases, and the risk that outranks them on a `risk: high` node.
            Blank reads "Same as Model", never the CLI's own default. */}
        {(
          [
            [CONTRACT_MODEL_LABEL, `contractModel`],
            [INTEGRATION_MODEL_LABEL, `integrationModel`],
            [RISK_MODEL_LABEL, `riskModel`],
          ] as const
        ).map(([label, field]) => (
          <Combobox
            key={field}
            triggerVariant="row"
            searchable={false}
            mobileTitle={label}
            disabled={readOnly}
            value={launch[field] || SAME_AS_MODEL}
            options={[
              { value: SAME_AS_MODEL, label: SAME_AS_MODEL_LABEL },
              ...agentModelValues(agent).map((value) => ({
                value,
                label: modelLabel(value),
              })),
            ]}
            onChange={(value) => {
              if (value !== null) {
                patchLaunch({
                  [field]: value === SAME_AS_MODEL ? null : value,
                })
              }
            }}
          />
        ))}
        {agentSupportsSubagentModel(agent) && (
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Subagent model"
            disabled={readOnly}
            value={launch.subagentModel || CLI_DEFAULT_MODEL}
            options={[
              { value: CLI_DEFAULT_MODEL, label: `Default` },
              ...contract.codingModel.values.map((value) => ({
                value,
                label: modelLabel(value),
              })),
            ]}
            onChange={(value) => {
              if (value !== null) {
                patchLaunch({
                  subagentModel: value === CLI_DEFAULT_MODEL ? null : value,
                })
              }
            }}
          />
        )}
        {/* EXP-1010: every node's PR gets an agent review, so the model it
            runs on is always a question (a `risk: high` node is never
            reviewed by its own model). */}
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle={REVIEW_MODEL_LABEL}
          disabled={readOnly}
          value={launch.reviewModel || CLI_DEFAULT_MODEL}
          options={[
            { value: CLI_DEFAULT_MODEL, label: `Default` },
            ...contract.codingModel.values.map((value) => ({
              value,
              label: modelLabel(value),
            })),
          ]}
          onChange={(value) => {
            if (value !== null) {
              patchLaunch({
                reviewModel: value === CLI_DEFAULT_MODEL ? null : value,
              })
            }
          }}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle={agent === `codex` ? `Reasoning` : `Effort`}
          disabled={readOnly}
          value={launch.effort || CLI_DEFAULT_EFFORT}
          options={[
            { value: CLI_DEFAULT_EFFORT, label: `CLI default` },
            ...agentEffortValues(agent).map((value) => ({
              value,
              label: effortLabel(value),
            })),
          ]}
          onChange={(value) => {
            if (value !== null) {
              patchLaunch({
                effort: value === CLI_DEFAULT_EFFORT ? null : value,
              })
            }
          }}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Max parallel"
          disabled={readOnly}
          value={String(launch.maxParallel ?? WORKFLOW_MAX_PARALLEL_DEFAULT)}
          options={Array.from(
            { length: WORKFLOW_MAX_PARALLEL_CAP },
            (_, index) => ({
              value: String(index + 1),
              label: String(index + 1),
            })
          )}
          onChange={(value) => {
            if (value !== null) patchLaunch({ maxParallel: Number(value) })
          }}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Start"
          disabled={readOnly}
          value={workflow.startOn}
          options={wfStartOnValues.map((value) => ({
            value: value as string,
            label: START_ON_LABELS[value] ?? value,
          }))}
          onChange={(value) => {
            if (value !== null) void onSave({ startOn: value as WfStartOn })
          }}
        />
      </GlassGroup>
    </section>
  )
}

/** The selected node: what it is, what it covers, the two picks the planner's
 *  numbers can be corrected with, and — once the workflow runs — the things
 *  only a person does: follow its run, approve its PR for the merge train,
 *  unstick it when it failed. */
export function WorkflowNodePanel({
  workflowId,
  node,
  nodes,
  issueById,
  teamSlug,
  workflowStatus,
  onError,
  onClose,
}: {
  workflowId: string
  node: WorkflowNode
  /** Every node of the workflow: `afterNodeIds` names NODES, and the panel
   *  shows the issues behind them. */
  nodes: readonly WorkflowNode[]
  issueById: ReadonlyMap<string, Issue>
  teamSlug: string
  workflowStatus: string
  onError: (message: string | null) => void
  onClose: () => void
}) {
  const boards = useTeamBoards(node.teamId)
  const [skipOpen, setSkipOpen] = useState(false)
  const issue = issueById.get(node.issueId)
  const members = node.memberIssueIds
    .map((id) => issueById.get(id))
    .filter((row): row is Issue => Boolean(row))
  const boardSlug = issue
    ? boards.find((board) => board.id === issue.boardId)?.slug
    : undefined
  // EXP-983: the nodes this one merges in first — the engine wrote them after
  // its work collided with theirs. A node whose issue has not synced is left
  // out rather than drawn as a uuid.
  const mergesFirst = node.afterNodeIds
    .map((id) => nodes.find((candidate) => candidate.id === id))
    .map((candidate) => {
      const row = candidate ? issueById.get(candidate.issueId) : undefined
      return row && candidate
        ? {
            ...row,
            identifier: workflowNodeTitle(
              row.identifier,
              candidate.memberIssueIds.length
            ),
          }
        : undefined
    })
    .filter((row): row is Issue => Boolean(row))

  const updateNode = async (patch: { kind?: WfNodeKind; risk?: WfRisk }) => {
    onError(null)
    try {
      await trpc.workflows.updateNode.mutate(
        { workflowId, issueId: node.issueId, ...patch },
        { context: { skipErrorToast: true } }
      )
    } catch (caught) {
      onError(trpcErrorMessage(caught, `The node could not be updated`))
    }
  }

  // EXP-984: a follow-up the run filed for itself. Admitting makes it a node
  // like any other; dismissing drops it — either way the server replans.
  const admit = async (admitted: boolean) => {
    onError(null)
    try {
      await trpc.workflows.admitNode.mutate(
        { nodeId: node.id, admit: admitted },
        { context: { skipErrorToast: true } }
      )
    } catch (caught) {
      onError(trpcErrorMessage(caught, `The node could not be admitted`))
    }
  }

  const approve = async (approved: boolean) => {
    onError(null)
    try {
      await trpc.workflows.approveNode.mutate(
        { nodeId: node.id, approved },
        { context: { skipErrorToast: true } }
      )
    } catch (caught) {
      onError(trpcErrorMessage(caught, `The node could not be approved`))
    }
  }

  const resolve = async (action: `retry` | `skip`) => {
    onError(null)
    setSkipOpen(false)
    try {
      await trpc.workflows.resolveNode.mutate(
        { nodeId: node.id, action },
        { context: { skipErrorToast: true } }
      )
    } catch (caught) {
      onError(trpcErrorMessage(caught, `The node could not be resolved`))
    }
  }

  // A proposal is not part of the run: nothing about it is approved, retried
  // or skipped until somebody admits it.
  const proposed = node.state === `proposed`
  // An approval only ever holds an OPEN pull request; once landed there is
  // nothing left to approve or take back. The agent review normally stamps
  // it; a person may, by hand.
  const needsApproval = !proposed && node.state === `in_review` && !node.approvedAt
  const canWithdraw =
    !proposed && Boolean(node.approvedAt) && node.state !== `landed`
  const review = readNodeReview(node.review)

  return (
    <div className="flex flex-col gap-3" data-testid="workflow-node-panel">
      {issue ? (
        <IssueChip
          issue={{
            ...issue,
            identifier: workflowNodeTitle(issue.identifier, members.length),
          }}
          preview={false}
          // The badge IS the way into the issue.
          link={
            boardSlug
              ? (props) => (
                  <Link
                    to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                    params={{
                      teamSlug,
                      boardSlug,
                      issueIdentifier: issue.identifier,
                    }}
                    onClick={onClose}
                    {...props}
                  />
                )
              : undefined
          }
          testId="workflow-node-issue"
        />
      ) : (
        <p className="text-xs text-muted-foreground">
          This issue has not synced yet.
        </p>
      )}
      {members.length > 0 && (
        <div className="flex flex-wrap gap-1.5" data-testid="workflow-node-members">
          {members.map((member) => (
            <IssueChip key={member.id} issue={member} preview={false} />
          ))}
        </div>
      )}
      {/* EXP-983: the node announced its contract, so its dependents may
          already be building on it. */}
      {node.checkpointAt && (
        <p
          className="text-xs text-muted-foreground"
          data-testid="workflow-node-checkpoint"
        >
          {`${CONTRACT_PUBLISHED_LABEL} ${relativeTime(node.checkpointAt)}`}
        </p>
      )}
      {mergesFirst.length > 0 && (
        <div className="flex flex-col gap-1.5" data-testid="workflow-node-merges-first">
          <span className="text-xs text-muted-foreground">
            {MERGES_IN_FIRST_LABEL}
          </span>
          <div className="flex flex-wrap gap-1.5">
            {mergesFirst.map((row) => (
              <IssueChip key={row.id} issue={row} preview={false} />
            ))}
          </div>
        </div>
      )}
      {/* Why the node is failed or waiting, in the engine's own words. */}
      {node.note && (
        <p className="text-xs text-muted-foreground" data-testid="workflow-node-note">
          {node.note}
        </p>
      )}
      {proposed && (
        <p
          className="text-xs text-muted-foreground"
          data-testid="workflow-node-proposed-note"
        >
          {PROPOSED_NODE_NOTE}
        </p>
      )}
      {review && <AgentReviewBlock review={review} />}
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Kind"
          // The kind shapes the PLAN: the server refuses it once the workflow
          // started (`updateNode`). Risk stays adjustable.
          disabled={workflowStatus !== `draft`}
          value={node.kind}
          options={wfNodeKindValues.map((value) => ({
            value: value as string,
            label: workflowNodeKindLabel(value),
          }))}
          onChange={(value) => {
            if (value !== null) void updateNode({ kind: value as WfNodeKind })
          }}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Risk"
          value={node.risk}
          options={wfRiskValues.map((value) => ({
            value: value as string,
            label: RISK_LABELS[value] ?? value,
          }))}
          onChange={(value) => {
            if (value !== null) void updateNode({ risk: value as WfRisk })
          }}
        />
      </GlassGroup>
      {node.touches.length > 0 && (
        <div
          className="flex flex-col gap-0.5"
          data-testid="workflow-node-touches"
        >
          {node.touches.map((glob) => (
            <span key={glob} className="truncate font-mono text-xs text-muted-foreground">
              {glob}
            </span>
          ))}
        </div>
      )}
      {/* EXP-1002: the node's surfaces read as the app's OWN face switcher —
          same labels, same order, same `availableFaces` rule as the Work
          screen — rather than a stack of differently-shaped buttons. Nothing
          is selected: the reader is on the graph, not on a face, so every
          segment is a way OUT of it. */}
      {issue && (
        <NodeFaceStrip
          teamSlug={teamSlug}
          boardSlug={boardSlug}
          issue={issue}
          sessionId={node.sessionId}
          onNavigate={onClose}
        />
      )}
      {needsApproval && (
        <Button
          size="sm"
          data-testid="workflow-node-approve"
          onClick={() => void approve(true)}
        >
          {APPROVE_NODE_LABEL}
        </Button>
      )}
      {canWithdraw && (
        <Button
          variant="outline"
          size="sm"
          data-testid="workflow-node-withdraw"
          onClick={() => void approve(false)}
        >
          {WITHDRAW_APPROVAL_LABEL}
        </Button>
      )}
      {proposed && (
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            data-testid="workflow-node-admit"
            onClick={() => void admit(true)}
          >
            {ADMIT_NODE_LABEL}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            data-testid="workflow-node-dismiss"
            onClick={() => void admit(false)}
          >
            {DISMISS_NODE_LABEL}
          </Button>
        </div>
      )}
      {/* A failed node's two ways out. */}
      {!proposed && node.state === `failed` && (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            data-testid="workflow-node-retry"
            onClick={() => void resolve(`retry`)}
          >
            {RETRY_NODE_LABEL}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            data-testid="workflow-node-skip"
            onClick={() => setSkipOpen(true)}
          >
            {SKIP_NODE_LABEL}
          </Button>
        </div>
      )}

      <Dialog open={skipOpen} onOpenChange={setSkipOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{SKIP_NODE_LABEL}</DialogTitle>
            <DialogDescription>{SKIP_NODE_CONFIRM}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel>Cancel</DialogCancel>
            <Button
              variant="destructive"
              data-testid="workflow-node-skip-confirm"
              onClick={() => void resolve(`skip`)}
            >
              {SKIP_NODE_LABEL}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// ── Agent review (EXP-984) ──────────────────────────────────────────────────

/** What the panel reads off the node's `review` jsonb. TOLERANT: the column is
 *  written by the engine, so a row from a newer server must never blank the
 *  panel out — anything unreadable simply reads as absent. */
interface PanelReview {
  verdict: string
  findings: string
  oracle: { command: string; passed: boolean } | null
  round: number
}

function readNodeReview(value: unknown): PanelReview | null {
  if (!value || typeof value !== `object` || Array.isArray(value)) return null
  const row = value as Record<string, unknown>
  if (typeof row.verdict !== `string` || !row.verdict) return null
  const raw = row.oracle
  const oracle =
    raw && typeof raw === `object` && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : null
  return {
    verdict: row.verdict,
    findings: typeof row.findings === `string` ? row.findings : ``,
    oracle:
      oracle && typeof oracle.command === `string` && oracle.command
        ? { command: oracle.command, passed: oracle.passed === true }
        : null,
    round:
      typeof row.round === `number` && Number.isFinite(row.round)
        ? row.round
        : 0,
  }
}

// ── The node's faces (EXP-1002) ─────────────────────────────────────────────

/** The face glyphs the phone switcher already uses, so one node reads the same
 *  wherever it is opened from. */
const FACE_ICON: Record<WorkFaceKind, ReturnType<typeof conceptIcon>> = {
  issue: conceptIcon(`ui-issue`),
  run: conceptIcon(`nav-devices`),
  changes: conceptIcon(`coding-diff`),
  results: conceptIcon(`work-results`),
}

/**
 * Issue · Run · Changes for the picked node, as the app's own segmented
 * control. Which segments exist is `availableFaces` — the SAME rule the Work
 * screen applies — so a node with no run shows no Run, and one with no pull
 * request shows no Changes. `results` never appears: the panel has no run
 * feed to publish from.
 *
 * Nothing is selected on purpose. The reader is on the graph, so the strip is
 * a way out of it rather than a picture of where they are; a value no segment
 * carries leaves them all inactive, which is exactly that.
 */
function NodeFaceStrip({
  teamSlug,
  boardSlug,
  issue,
  sessionId,
  onNavigate,
}: {
  teamSlug: string
  boardSlug: string | undefined
  issue: Issue
  sessionId: string | null
  onNavigate: () => void
}) {
  const navigate = useNavigate()
  const faces = availableFaces({
    hasIssue: Boolean(boardSlug),
    hasRun: Boolean(sessionId),
    hasChanges: issue.prNumber != null,
    hasResults: false,
  })
  if (faces.length === 0) return null

  const go = (face: WorkFaceKind) => {
    onNavigate()
    if (face === `run` && sessionId) {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId },
      })
      return
    }
    if (face === `changes`) {
      void navigate({
        to: `/t/$teamSlug/reviews/$issueIdentifier`,
        params: { teamSlug, issueIdentifier: issue.identifier },
      })
      return
    }
    if (boardSlug) {
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug, boardSlug, issueIdentifier: issue.identifier },
      })
    }
  }

  return (
    <div data-testid="workflow-node-faces">
      <SegmentedControl
        fill
        value=""
        onValueChange={(face) => go(face as WorkFaceKind)}
        options={faces.map((face) => ({
          value: face,
          label: faceLabel(face),
          icon: FACE_ICON[face],
        }))}
      />
    </div>
  )
}

/** Long findings fold behind "Show more" — the agent-session rule, on the
 *  panel's smaller budget. */
const FINDINGS_CLAMP_LINES = 4
const FINDINGS_CLAMP_CHARS = 280

/** The reviewer's latest verdict: the one line (green for an approval, red for
 *  requested changes), its findings, and the check it actually ran. */
function AgentReviewBlock({ review }: { review: PanelReview }) {
  const [expanded, setExpanded] = useState(false)
  const approved = review.verdict === `approve`
  const long =
    review.findings.length > FINDINGS_CLAMP_CHARS ||
    review.findings.split(`\n`).length > FINDINGS_CLAMP_LINES
  return (
    <div className="flex flex-col gap-1" data-testid="workflow-node-review">
      <span className="text-xs text-muted-foreground">{AGENT_REVIEW_TITLE}</span>
      <span
        className={cn(
          `text-xs`,
          approved ? `text-emerald-500` : `text-destructive`
        )}
        data-testid="workflow-node-review-line"
      >
        {workflowReviewLine(review)}
      </span>
      {review.findings && (
        <p
          className={cn(
            `text-xs whitespace-pre-wrap text-muted-foreground`,
            long && !expanded && `line-clamp-4`
          )}
          data-testid="workflow-node-review-findings"
        >
          {review.findings}
        </p>
      )}
      {long && (
        <Button
          variant="text"
          size="inline"
          className="self-start font-medium"
          data-testid="workflow-node-review-more"
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? `Show less` : `Show more`}
        </Button>
      )}
      {review.oracle && (
        <span
          className="truncate font-mono text-xs text-muted-foreground"
          data-testid="workflow-node-review-oracle"
        >
          {review.oracle.command}
        </span>
      )}
    </div>
  )
}
