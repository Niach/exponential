import { useEffect, useMemo, useState } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import {
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
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  useIsMobile,
} from "@exp/ui"
import {
  WORKFLOW_MAX_PARALLEL_CAP,
  WORKFLOW_MAX_PARALLEL_DEFAULT,
  wfGateValues,
  wfNodeKindValues,
  wfRiskValues,
  wfStartOnValues,
  type WfGate,
  type WfNodeKind,
  type WfRisk,
  type WfStartOn,
  type WorkflowLaunch,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import type { Issue, SyncedWorkflow, WorkflowNode } from "@/db/schema"
import { IssueChip } from "@/components/issue-chip"
import { WorkflowGraph } from "@/components/workflow-graph"
import {
  CLI_DEFAULT_EFFORT,
  CLI_DEFAULT_MODEL,
  effortLabel,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import { useWorkflowNodes } from "@/hooks/use-workflows"
import { agentHealth, healthBadgeLabel, SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
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
  workflowNodeKindLabel,
  workflowNodeNeedsApproval,
  workflowShapeLine,
  workflowNodeTitle,
  workflowStartBlocker,
  workflowTrainStepLabel,
  APPROVE_NODE_LABEL,
  CANCEL_WORKFLOW_CONFIRM,
  CANCEL_WORKFLOW_LABEL,
  DELETE_WORKFLOW_LABEL,
  MERGE_TRAIN_EMPTY,
  MERGE_TRAIN_TITLE,
  PAUSE_WORKFLOW_LABEL,
  PLAN_WORKFLOW_LABEL,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
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

const WorkflowIcon = conceptIcon(`nav-workflows`)
const DeleteIcon = conceptIcon(`ui-delete`)
const PlanIcon = conceptIcon(`action-run`)
const StartIcon = conceptIcon(`action-run`)
const ResumeIcon = conceptIcon(`run-resume`)
const CancelIcon = conceptIcon(`ui-stop`)

/** The run configuration's labels. Byte-identical ×4 (the brief's words). */
const GATE_LABELS: Record<string, string> = {
  none: `No gate`,
  agent: `Agent review`,
  human: `Human review`,
}
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
  gate?: WfGate
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
      issueById={issueById}
      teamSlug={teamSlug}
      gate={workflow.gate}
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
          gate={workflow.gate}
          issueById={issueById}
          stacked={isMobile}
        />
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
  gate,
  issueById,
  stacked,
}: {
  nodes: readonly WorkflowNode[]
  gate: string
  issueById: ReadonlyMap<string, Issue>
  /** Phones read the train as a short list rather than a horizontal strip. */
  stacked: boolean
}) {
  const entries = workflowMergeTrain(nodes, gate)
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
  const patchLaunch = (patch: Partial<WorkflowLaunch>) =>
    void onSave({ launch: { ...launch, ...patch } })

  // The machine's logins for the picked agent, the active one first — the
  // composer's rule (EXP-825/849), read straight off the heartbeat.
  const accountProfiles = (device?.agentAccounts?.[agent]?.profiles ?? [])
    .filter((profile) => Boolean(profile?.id))
    .map((profile) => ({
      id: profile.id,
      label:
        profile.label ||
        (profile.id === SYSTEM_PROFILE_ID ? `Default` : profile.id),
      active: profile.active === true,
      health: agentHealth(profile),
    }))
    .sort((left, right) => Number(right.active) - Number(left.active))

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
            // A different agent has a different model/effort vocabulary —
            // stale values would only be refused by the router.
            if (value !== null) {
              patchLaunch({
                agent: value,
                model: null,
                effort: null,
                subagentModel: null,
              })
            }
          }}
        />
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
        {accountProfiles.length >= 2 && (
          <Combobox
            triggerVariant="row"
            searchable={false}
            mobileTitle="Account"
            disabled={readOnly}
            value={launch.account ?? null}
            triggerLabel="Machine default"
            options={accountProfiles.map((profile) => ({
              value: profile.id,
              // EXP-849: a dead credential says so BEFORE the run lands on it.
              label: healthBadgeLabel(profile.health)
                ? `${profile.label} — ${healthBadgeLabel(profile.health)}`
                : profile.label,
            }))}
            onChange={(value) => {
              if (value !== null) patchLaunch({ account: value })
            }}
          />
        )}
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
          mobileTitle="Gate"
          disabled={readOnly}
          value={workflow.gate}
          options={wfGateValues.map((value) => ({
            value: value as string,
            label: GATE_LABELS[value] ?? value,
          }))}
          onChange={(value) => {
            if (value !== null) void onSave({ gate: value as WfGate })
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
  issueById,
  teamSlug,
  gate,
  workflowStatus,
  onError,
  onClose,
}: {
  workflowId: string
  node: WorkflowNode
  issueById: ReadonlyMap<string, Issue>
  teamSlug: string
  /** The workflow's `gate` — with the node's kind it decides whether landing
   *  waits for a person (`workflowNodeNeedsApproval`). */
  gate: string
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

  // The gate only ever holds an OPEN pull request; once landed there is
  // nothing left to approve or take back.
  const needsApproval =
    node.state === `in_review` &&
    workflowNodeNeedsApproval(gate, node.kind) &&
    !node.approvedAt
  const canWithdraw = Boolean(node.approvedAt) && node.state !== `landed`

  return (
    <div className="flex flex-col gap-3" data-testid="workflow-node-panel">
      {issue ? (
        <IssueChip
          issue={{
            ...issue,
            identifier: workflowNodeTitle(issue.identifier, members.length),
          }}
          preview={false}
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
      {/* Why the node is failed or waiting, in the engine's own words. */}
      {node.note && (
        <p className="text-xs text-muted-foreground" data-testid="workflow-node-note">
          {node.note}
        </p>
      )}
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
      {issue && boardSlug && (
        <Button variant="outline" size="sm" asChild onClick={onClose}>
          <Link
            to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
            params={{
              teamSlug,
              boardSlug,
              issueIdentifier: issue.identifier,
            }}
          >
            Open issue
          </Link>
        </Button>
      )}
      {/* The node's own run, steered on the ONE run URL (EXP-870). */}
      {node.sessionId && (
        <Button
          variant="outline"
          size="sm"
          asChild
          onClick={onClose}
          data-testid="workflow-node-run"
        >
          <Link
            to="/t/$teamSlug/sessions/$sessionId"
            params={{ teamSlug, sessionId: node.sessionId }}
          >
            Open run
          </Link>
        </Button>
      )}
      {issue?.prNumber != null && (
        <Button
          variant="outline"
          size="sm"
          asChild
          onClick={onClose}
          data-testid="workflow-node-pr"
        >
          <Link
            to="/t/$teamSlug/reviews/$issueIdentifier"
            params={{ teamSlug, issueIdentifier: issue.identifier }}
          >
            {`PR #${issue.prNumber}`}
          </Link>
        </Button>
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
      {node.state === `failed` && (
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
