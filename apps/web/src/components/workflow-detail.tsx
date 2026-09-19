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
  agentAllowsBlankModel,
  agentSupportsSubagentModel,
} from "@/lib/coding-launch-prefs"
import { workflowCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  workflowCycleNote,
  workflowNodeKindLabel,
  workflowShapeLine,
  workflowNodeTitle,
  DELETE_WORKFLOW_LABEL,
  PLAN_WORKFLOW_LABEL,
} from "@/lib/workflow-view"

// EXP-981: ONE draft workflow — its name, the shape line, the graph, the node
// panel beside it and the start configuration under it. P2 has NO Start
// button: the engine (and with it start/pause/cancel) lands next. Every write
// is a `workflows.*` mutation whose txId the synced collection echoes back, so
// the page never keeps a copy of the row.

const WorkflowIcon = conceptIcon(`nav-workflows`)
const DeleteIcon = conceptIcon(`ui-delete`)
const PlanIcon = conceptIcon(`action-run`)

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
  const panel = selectedNode ? (
    <WorkflowNodePanel
      workflowId={workflow.id}
      node={selectedNode}
      issueById={issueById}
      teamSlug={teamSlug}
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

      <HowItRunsSection workflow={workflow} onSave={save} />

      <div className="flex flex-wrap items-center gap-2">
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
        <Button
          variant="ghost"
          className="text-destructive hover:text-destructive"
          data-testid="workflow-delete"
          onClick={() => setDeleteOpen(true)}
        >
          <DeleteIcon className="size-4" />
          {DELETE_WORKFLOW_LABEL}
        </Button>
      </div>

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

  return (
    <section className="flex flex-col" data-testid="workflow-how-it-runs">
      <GlassSectionHeader label="How it runs" />
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Runner device"
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
          value={launch.model || CLI_DEFAULT_MODEL}
          options={[
            ...(agentAllowsBlankModel(agent)
              ? [{ value: CLI_DEFAULT_MODEL, label: `CLI default` }]
              : []),
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

/** The selected node: what it is, what it covers, and the two picks the
 *  planner's numbers can be corrected with. */
export function WorkflowNodePanel({
  workflowId,
  node,
  issueById,
  teamSlug,
  onError,
  onClose,
}: {
  workflowId: string
  node: WorkflowNode
  issueById: ReadonlyMap<string, Issue>
  teamSlug: string
  onError: (message: string | null) => void
  onClose: () => void
}) {
  const boards = useTeamBoards(node.teamId)
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
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Kind"
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
    </div>
  )
}
