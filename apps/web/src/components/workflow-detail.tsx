import { useEffect, useMemo, useState, useSyncExternalStore } from "react"
import { useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  Alert,
  AlertDescription,
  Button,
  categoryStatusIcon,
  BUILTIN_STATUS_COLOR_CLASS,
  conceptIcon,
  DevicePicker,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DisclosureHeader,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Input,
  IssueChip as IssueChipView,
  IssueChipStack,
  LiveDot,
  parseSessionResults,
  PrGithubButton,
  SessionResultsView,
  StatusGlyph,
  WorkflowEventList,
  type DevicePickerDevice,
  type StatusGlyphProps,
} from "@exp/ui"
import type {
  Board,
  CodingSession,
  Issue,
  IssueLabel,
  SyncedWorkflow,
  WorkflowEvent,
  WorkflowNode,
} from "@/db/schema"
import {
  CHANGES_FACE_LABEL,
  ISSUE_FACE_LABEL,
  RESULTS_FACE_LABEL,
  RUNS_FACE_LABEL,
} from "@/lib/work-faces"
import {
  WorkFaceToggle,
  type WorkFace,
  type WorkFaceItem,
} from "@/components/team/work-face-toggle"
import { AgentSessionView } from "@/components/agent-session"
import { ChangesView } from "@/components/changes-view"
import { IssueBlocksPopover } from "@/components/issue-blocks-badge"
import { IssueDetailView } from "@/components/issue-detail-view"
import { IssueList } from "@/components/issue-list"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { SessionTree } from "@/components/session-tree"
import { SteerComposer } from "@/components/steer-composer"
import {
  ALL_SELECTION,
  pruneSelection,
  selectNode,
  stepSelection,
  stripStepKey,
  workflowBody,
  type StripSelection,
  type WorkflowFace,
} from "@/components/workflow-detail-selection"
import {
  rowPrState,
  useSessionListRows,
  useSessionRow,
} from "@/hooks/use-agents-data"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useReviewFiles } from "@/hooks/use-review-files"
import { useSession } from "@/hooks/use-session"
import {
  useTeamBoards,
  useTeamBySlug,
  useTeamLabels,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import { useWorkflowNodes } from "@/hooks/use-workflows"
import {
  buildIssueLabelMap,
  buildVisibleIssueGroups,
  nestIssueGroups,
} from "@/lib/board-view"
import {
  codingSessionCollection,
  issueLabelCollection,
  workflowCollection,
  workflowEventCollection,
} from "@/lib/collections"
import { sessionIdentity } from "@/lib/session-identity"
import { acquireSteerSession } from "@/lib/steer-session-store"
import { deviceIsOnline } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { cn } from "@/lib/utils"
import {
  workflowOpenQuestions,
  type WorkflowOpenQuestion,
} from "@/lib/workflows/open-questions"
import {
  ADMIT_NODE_LABEL,
  CANCEL_WORKFLOW_CONFIRM,
  CANCEL_WORKFLOW_LABEL,
  DELETE_WORKFLOW_LABEL,
  DISMISS_NODE_LABEL,
  FINAL_PR_TITLE,
  MERGE_FINAL_PR_CONFIRM,
  MERGE_FINAL_PR_LABEL,
  NEEDS_YOU_LABEL,
  NODE_UNSYNCED_TITLE,
  PAUSE_WORKFLOW_LABEL,
  RESUME_WORKFLOW_LABEL,
  RETRY_NODE_LABEL,
  SKIP_NODE_CONFIRM,
  SKIP_NODE_LABEL,
  START_WORKFLOW_LABEL,
  nodeChipMenu,
  workflowCycleNote,
  workflowEdges,
  workflowHeaderCaption,
  workflowNodeDisplayState,
  workflowNodeStrip,
  workflowPrimaryAction,
  workflowStartBlocker,
  type NodeChip,
  type NodeChipMenuItem,
  type WfNodeDisplayState,
  type WorkflowPrimaryAction,
} from "@/lib/workflow-view"

// EXP-1084: ONE workflow is ONE place for all of its issues, runs, changes and
// results. The header (name, status, the ONE primary action, an overflow with
// Stop and Delete, the caption line), the NODE STRIP as the picker (`All`, then
// every node in DAG order), the Work screen's face toggle under it, and the
// body = selection × face. Nothing on this page configures the run; every
// write is a `workflows.*` mutation whose echo the synced rows carry back.

const MoreIcon = conceptIcon(`ui-more`)
const StartIcon = conceptIcon(`action-run`)
const ResumeIcon = conceptIcon(`run-resume`)

/** The picker's first chip. */
export const ALL_NODES_LABEL = `All`
/** The collapsed log under the All × Issue list. */
export const DECISIONS_LABEL = `Decisions`
const SELECT_DEVICE_LABEL = `Select a device`

type WorkflowIntent = `start` | `pause` | `resume` | `cancel`
const INTENT_ERROR: Record<WorkflowIntent, string> = {
  start: `The workflow could not be started`,
  pause: `The workflow could not be paused`,
  resume: `The workflow could not be resumed`,
  cancel: `The workflow could not be cancelled`,
}

const NODE_MENU_LABEL: Record<NodeChipMenuItem, string> = {
  retry: RETRY_NODE_LABEL,
  skip: SKIP_NODE_LABEL,
  admit: ADMIT_NODE_LABEL,
  dismiss: DISMISS_NODE_LABEL,
}

/** A node's display state as the issue-status glyph it reads like. */
export function nodeDisplayGlyph(display: WfNodeDisplayState): StatusGlyphProps {
  switch (display) {
    case `running`:
      return {
        icon: categoryStatusIcon(`started`, 0, 2),
        colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
      }
    case `done`:
      return {
        icon: categoryStatusIcon(`completed`, 0, 0),
        colorClass: BUILTIN_STATUS_COLOR_CLASS.done,
      }
    case `failed`:
      return {
        icon: categoryStatusIcon(`cancelled`, 0, 0),
        colorClass: `text-destructive`,
      }
    case `skipped`:
      return {
        icon: categoryStatusIcon(`cancelled`, 0, 0),
        colorClass: BUILTIN_STATUS_COLOR_CLASS.cancelled,
      }
    default:
      return {
        icon: categoryStatusIcon(`backlog`, 0, 0),
        colorClass: BUILTIN_STATUS_COLOR_CLASS.backlog,
      }
  }
}

/** The header's workflow status glyph, in the same vocabulary. */
function workflowStatusGlyph(status: string): StatusGlyphProps {
  if (status === `running` || status === `paused`) return nodeDisplayGlyph(`running`)
  if (status === `done`) return nodeDisplayGlyph(`done`)
  if (status === `failed`) return nodeDisplayGlyph(`failed`)
  if (status === `cancelled`) return nodeDisplayGlyph(`skipped`)
  return nodeDisplayGlyph(`queued`)
}

/** The issues a node covers: its representative, then a compound node's
 *  sub-issues. */
function nodeIssueIds(node: WorkflowNode): string[] {
  return [node.issueId, ...node.memberIssueIds]
}

/** The runs that belong to the picked nodes (all of the workflow's on `All`). */
export function scopeSessions(
  sessions: readonly CodingSession[],
  picked: readonly WorkflowNode[]
): CodingSession[] {
  if (picked.length === 0) return [...sessions]
  const nodeIds = new Set(picked.map((node) => node.id))
  const sessionIds = new Set(
    picked.map((node) => node.sessionId).filter((id): id is string => Boolean(id))
  )
  return sessions.filter(
    (session) =>
      sessionIds.has(session.id) ||
      (session.workflowNodeId != null && nodeIds.has(session.workflowNodeId))
  )
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return (
    target.closest(`input, textarea, select, [contenteditable="true"], [role="dialog"], [role="menu"]`) !==
    null
  )
}

export function WorkflowDetail({
  workflow,
  teamSlug,
  initialFace = `issue`,
  initialNodeId,
}: {
  workflow: SyncedWorkflow
  teamSlug: string
  initialFace?: WorkflowFace
  initialNodeId?: string
}) {
  const navigate = useNavigate()
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const teamId = workflow.teamId
  const nodes = useWorkflowNodes(workflow.id)
  const graph = useTeamIssueGraph(teamId)
  const issueById = useMemo(
    () => new Map(graph.issues.map((issue) => [issue.id, issue])),
    [graph.issues]
  )
  const nodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes]
  )

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) => eq(s.workflowId, workflow.id)),
    [workflow.id]
  )
  const sessions = useMemo(
    () => (sessionRows ?? []) as CodingSession[],
    [sessionRows]
  )
  const sessionById = useMemo(
    () => new Map(sessions.map((session) => [session.id, session])),
    [sessions]
  )
  const questions = useMemo(
    () => workflowOpenQuestions(sessions, workflow.id),
    [sessions, workflow.id]
  )

  // The runner: the header caption names it, a draft without one picks it.
  const remote = useRemoteStart({ currentUserId, teamId })
  const runner = (remote.devices ?? []).find(
    (device) => device.deviceId === workflow.deviceId
  )
  const deviceLabel = workflow.deviceId
    ? runner?.deviceLabel || workflow.deviceId
    : null

  const strip = useMemo(
    () =>
      workflowNodeStrip(
        nodes.map((node) => {
          const session = node.sessionId ? sessionById.get(node.sessionId) : undefined
          return {
            id: node.id,
            identifier:
              issueById.get(node.issueId)?.identifier ?? node.issueId.slice(0, 8),
            state: node.state,
            wave: node.wave,
            lane: node.lane,
            members: node.memberIssueIds.length,
            live: session?.agentBusy === true,
            needsYou: questions.some((question) => question.nodeId === node.id),
            note: node.note,
          }
        }),
        workflowEdges(nodes, graph.relations).map(
          (edge) => [edge.from, edge.to] as [string, string]
        )
      ),
    [nodes, sessionById, issueById, questions, graph.relations]
  )
  const order = useMemo(
    () => strip.flatMap((wave) => wave.nodes.map((chip) => chip.id)),
    [strip]
  )

  const [selection, setSelection] = useState<StripSelection>(() =>
    initialNodeId
      ? { ids: [initialNodeId], anchor: initialNodeId, cursor: initialNodeId }
      : ALL_SELECTION
  )
  const [face, setFace] = useState<WorkflowFace>(initialFace)
  const [error, setError] = useState<string | null>(null)
  const [dialog, setDialog] = useState<
    `cancel` | `delete` | `merge` | { skip: string } | null
  >(null)

  // A node a replan folded away must not stay picked. Only once the nodes
  // synced: an initial `?node=` would otherwise be pruned on the first frame.
  useEffect(() => {
    if (order.length > 0) setSelection((current) => pruneSelection(current, order))
  }, [order])

  // ←/→ and j/k step through All + the nodes, keeping the face.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableTarget(event.target)) return
      const step = stripStepKey(event)
      if (step === null) return
      event.preventDefault()
      setSelection((current) => stepSelection(current, step, order))
    }
    document.addEventListener(`keydown`, onKey)
    return () => document.removeEventListener(`keydown`, onKey)
  }, [order])

  const picked = selection.ids
    .map((id) => nodeById.get(id))
    .filter((node): node is WorkflowNode => Boolean(node))
  const body = workflowBody(face, picked.length)

  const run = async (
    label: string,
    call: () => Promise<{ txId?: number } | unknown>
  ) => {
    setError(null)
    try {
      const result = (await call()) as { txId?: number } | undefined
      if (result && typeof result.txId === `number`) {
        await workflowCollection.utils.awaitTxId(result.txId)
      }
    } catch (caught) {
      setError(trpcErrorMessage(caught, label))
    }
  }
  const quiet = { context: { skipErrorToast: true } }

  const intent = (which: WorkflowIntent) =>
    run(INTENT_ERROR[which], () => {
      const input = { id: workflow.id }
      if (which === `start`) return trpc.workflows.start.mutate(input, quiet)
      if (which === `pause`) return trpc.workflows.pause.mutate(input, quiet)
      if (which === `resume`) return trpc.workflows.resume.mutate(input, quiet)
      return trpc.workflows.cancel.mutate(input, quiet)
    })

  const save = (patch: { name?: string; deviceId?: string | null }) =>
    run(`The workflow could not be updated`, () =>
      trpc.workflows.update.mutate({ id: workflow.id, ...patch }, quiet)
    )

  const remove = async () => {
    setDialog(null)
    setError(null)
    try {
      await trpc.workflows.delete.mutate({ id: workflow.id }, quiet)
      void navigate({ to: `/t/$teamSlug/workflows`, params: { teamSlug } })
    } catch (caught) {
      setError(trpcErrorMessage(caught, `The workflow could not be deleted`))
    }
  }

  const nodeAction = (nodeId: string, item: NodeChipMenuItem) => {
    if (item === `skip`) {
      setDialog({ skip: nodeId })
      return
    }
    if (item === `retry`) {
      void run(`The node could not be retried`, () =>
        trpc.workflows.resolveNode.mutate({ nodeId, action: `retry` }, quiet)
      )
      return
    }
    void run(`The node could not be admitted`, () =>
      trpc.workflows.admitNode.mutate({ nodeId, admit: item === `admit` }, quiet)
    )
  }

  const primary = workflowPrimaryAction(workflow.status, deviceLabel)
  const cycleNote = workflowCycleNote(workflow.metrics)
  const startBlocker =
    workflow.status === `draft` && primary === `start`
      ? workflowStartBlocker(workflow, workflow.metrics)
      : null
  const notice = workflow.status === `draft` ? (cycleNote ?? startBlocker) : null
  const live = workflow.status === `running` || workflow.status === `paused`

  const runnerDevices: DevicePickerDevice[] = (remote.devices ?? [])
    .filter(
      (device) =>
        device.deviceId === workflow.deviceId ||
        (deviceIsOnline(device) && (device.caps ?? []).includes(`workflows`))
    )
    .map((device) => ({
      id: device.deviceId,
      name: `${device.deviceLabel || device.deviceId}${
        device.owner ? ` — ${device.owner.name}` : ``
      }`,
      icon: device.icon,
      kind: device.kind,
    }))

  const faceItems: WorkFaceItem[] = (
    [
      [`issue`, `issue`, ISSUE_FACE_LABEL],
      [`runs`, `run`, RUNS_FACE_LABEL],
      [`changes`, `diff`, CHANGES_FACE_LABEL],
      [`results`, `results`, RESULTS_FACE_LABEL],
    ] as const
  ).map(([kind, toggleFace, label]) => ({
    face: toggleFace,
    label,
    onSelect: () => setFace(kind),
  }))
  const toggleFace: WorkFace =
    face === `runs` ? `run` : face === `changes` ? `diff` : face

  const scopeNodes = picked.length > 0 ? picked : nodes
  const scopedSessions = scopeSessions(sessions, picked)
  const skipNodeId =
    dialog && typeof dialog === `object` ? dialog.skip : null

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="workflow-detail">
      <div className="flex shrink-0 flex-col gap-3 px-4 pt-4 pb-2">
        {questions.length > 0 && (
          <OpenQuestionsBanner
            questions={questions}
            sessionById={sessionById}
            nodeById={nodeById}
            issueById={issueById}
            currentUserId={currentUserId}
            teamId={teamId}
            onPick={(nodeId) =>
              setSelection(selectNode(ALL_SELECTION, nodeId, order))
            }
          />
        )}

        <header className="flex flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-2">
            <StatusGlyph
              {...workflowStatusGlyph(workflow.status)}
              className="size-4 shrink-0"
            />
            <WorkflowNameField
              name={workflow.name}
              onRename={(name) => void save({ name })}
            />
            <PrimaryAction
              action={primary}
              startDisabled={startBlocker !== null}
              devices={runnerDevices}
              deviceId={workflow.deviceId}
              onPickDevice={(deviceId) => void save({ deviceId })}
              onIntent={(which) => void intent(which)}
              onReview={() =>
                void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
              }
            />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label="More"
                  data-testid="workflow-more"
                >
                  <MoreIcon className="size-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {live ? (
                  <DropdownMenuItem
                    variant="destructive"
                    data-testid="workflow-cancel"
                    onSelect={() => setDialog(`cancel`)}
                  >
                    {CANCEL_WORKFLOW_LABEL}
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem
                    variant="destructive"
                    data-testid="workflow-delete"
                    onSelect={() => setDialog(`delete`)}
                  >
                    {DELETE_WORKFLOW_LABEL}
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <p
            className="truncate pl-7 text-xs text-muted-foreground"
            data-testid="workflow-caption"
          >
            {workflowHeaderCaption(
              workflow.status,
              nodes.map((node) => ({
                state: node.state,
                members: node.memberIssueIds.length,
              })),
              deviceLabel
            )}
          </p>
        </header>

        {notice && (
          <Alert data-testid="workflow-notice">
            <AlertDescription>{notice}</AlertDescription>
          </Alert>
        )}
        {error && (
          <Alert variant="destructive" data-testid="workflow-error">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <NodeStrip
          strip={strip}
          nodeById={nodeById}
          issueById={issueById}
          teamId={teamId}
          selection={selection}
          onSelect={(id, modifiers) =>
            setSelection((current) => selectNode(current, id, order, modifiers))
          }
          onMenu={nodeAction}
        />

        <WorkFaceToggle face={toggleFace} items={faceItems} />
      </div>

      <div className="flex min-h-0 flex-1 flex-col" data-testid={`workflow-body-${body}`}>
        {body === `issue-detail` && picked[0] ? (
          <NodeIssueFace
            issue={issueById.get(picked[0].issueId)}
            teamSlug={teamSlug}
          />
        ) : body === `issue-list` ? (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <WorkflowIssueList
              teamSlug={teamSlug}
              teamId={teamId}
              issueIds={scopeNodes.flatMap(nodeIssueIds)}
              issueById={issueById}
              relations={graph.relations}
              graph={graph}
              nodes={nodes}
              onPickNode={(nodeId) =>
                setSelection(selectNode(ALL_SELECTION, nodeId, order))
              }
            />
            {picked.length === 0 && <DecisionsLog workflow={workflow} />}
          </div>
        ) : body === `runs` ? (
          <RunsFace
            key={selection.ids.join(`,`)}
            teamId={teamId}
            teamSlug={teamSlug}
            sessions={scopedSessions}
            defaultRunId={picked.length === 1 ? picked[0]!.sessionId : null}
            currentUserId={currentUserId}
          />
        ) : body === `changes` ? (
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
            {picked.length === 0 && workflow.finalPrUrl && (
              <div
                className="flex items-center gap-2 py-2"
                data-testid="workflow-final-pr"
              >
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {`${FINAL_PR_TITLE} #${workflow.finalPrNumber ?? ``}`}
                </span>
                <PrGithubButton prUrl={workflow.finalPrUrl} />
                {workflow.finalPrState === `open` && (
                  <Button
                    size="sm"
                    data-testid="workflow-final-pr-merge"
                    onClick={() => setDialog(`merge`)}
                  >
                    {MERGE_FINAL_PR_LABEL}
                  </Button>
                )}
              </div>
            )}
            {scopeNodes.map((node) => (
              <NodeChanges
                key={node.id}
                node={node}
                issue={issueById.get(node.issueId)}
                single={scopeNodes.length === 1}
              />
            ))}
          </div>
        ) : (
          <ResultsFace sessions={scopedSessions} />
        )}
      </div>

      <ConfirmDialog
        open={dialog === `merge`}
        onClose={() => setDialog(null)}
        title={`${MERGE_FINAL_PR_LABEL} ${FINAL_PR_TITLE.toLowerCase()}`}
        description={MERGE_FINAL_PR_CONFIRM}
        confirm={MERGE_FINAL_PR_LABEL}
        testId="workflow-final-pr-merge-confirm"
        onConfirm={() => {
          setDialog(null)
          void run(`The final pull request could not be merged`, () =>
            trpc.workflows.mergeFinalPr.mutate({ id: workflow.id }, quiet)
          )
        }}
      />
      <ConfirmDialog
        open={dialog === `cancel`}
        onClose={() => setDialog(null)}
        title={CANCEL_WORKFLOW_LABEL}
        description={CANCEL_WORKFLOW_CONFIRM}
        confirm={CANCEL_WORKFLOW_LABEL}
        destructive
        testId="workflow-cancel-confirm"
        onConfirm={() => {
          setDialog(null)
          void intent(`cancel`)
        }}
      />
      <ConfirmDialog
        open={dialog === `delete`}
        onClose={() => setDialog(null)}
        title={DELETE_WORKFLOW_LABEL}
        description={`"${workflow.name}" and its plan are deleted. The issues themselves stay where they are.`}
        confirm={DELETE_WORKFLOW_LABEL}
        destructive
        testId="workflow-delete-confirm"
        onConfirm={() => void remove()}
      />
      <ConfirmDialog
        open={skipNodeId !== null}
        onClose={() => setDialog(null)}
        title={SKIP_NODE_LABEL}
        description={SKIP_NODE_CONFIRM}
        confirm={SKIP_NODE_LABEL}
        destructive
        testId="workflow-node-skip-confirm"
        onConfirm={() => {
          setDialog(null)
          if (skipNodeId) {
            void run(`The node could not be skipped`, () =>
              trpc.workflows.resolveNode.mutate(
                { nodeId: skipNodeId, action: `skip` },
                quiet
              )
            )
          }
        }}
      />
    </div>
  )
}

// ── Header ──────────────────────────────────────────────────────────────────

/** The name, edited in place: saves on blur (the issue title's rule) and never
 *  fights an Electric echo while it has focus. */
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
      className="h-8 min-w-0 flex-1 border-0 bg-transparent px-1 text-base font-semibold shadow-none focus-visible:ring-0"
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

/** The header's ONE primary button (`workflowPrimaryAction`). */
function PrimaryAction({
  action,
  startDisabled,
  devices,
  deviceId,
  onPickDevice,
  onIntent,
  onReview,
}: {
  action: WorkflowPrimaryAction | null
  startDisabled: boolean
  devices: DevicePickerDevice[]
  deviceId: string | null
  onPickDevice: (deviceId: string) => void
  onIntent: (which: `start` | `pause` | `resume`) => void
  onReview: () => void
}) {
  switch (action) {
    case `pick_device`:
      return (
        <span className="inline-flex shrink-0" data-testid="workflow-device">
          <DevicePicker
            mobileTitle={SELECT_DEVICE_LABEL}
            value={deviceId}
            devices={devices}
            onChange={onPickDevice}
            trigger={<Button size="sm">{SELECT_DEVICE_LABEL}</Button>}
          />
        </span>
      )
    case `start`:
      return (
        <Button
          size="sm"
          data-testid="workflow-start"
          disabled={startDisabled}
          onClick={() => onIntent(`start`)}
        >
          <StartIcon className="size-4" />
          {START_WORKFLOW_LABEL}
        </Button>
      )
    case `pause`:
      return (
        <Button
          size="sm"
          variant="outline"
          data-testid="workflow-pause"
          onClick={() => onIntent(`pause`)}
        >
          {PAUSE_WORKFLOW_LABEL}
        </Button>
      )
    case `resume`:
      return (
        <Button
          size="sm"
          data-testid="workflow-resume"
          onClick={() => onIntent(`resume`)}
        >
          <ResumeIcon className="size-4" />
          {RESUME_WORKFLOW_LABEL}
        </Button>
      )
    case `review_final_pr`:
      return (
        <Button size="sm" data-testid="workflow-review" onClick={onReview}>
          {FINAL_PR_TITLE}
        </Button>
      )
    default:
      return null
  }
}

function ConfirmDialog({
  open,
  onClose,
  title,
  description,
  confirm,
  destructive = false,
  testId,
  onConfirm,
}: {
  open: boolean
  onClose: () => void
  title: string
  description: string
  confirm: string
  destructive?: boolean
  testId: string
  onConfirm: () => void
}) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogCancel>Cancel</DialogCancel>
          <Button
            variant={destructive ? `destructive` : `default`}
            data-testid={testId}
            onClick={onConfirm}
          >
            {confirm}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ── The node strip ──────────────────────────────────────────────────────────

/** One node as the chip every surface names an issue with — stacked for a
 *  compound node, its display state as the glyph. */
function NodeChipView({ chip, issue }: { chip: NodeChip; issue: Issue | undefined }) {
  const view = (
    <IssueChipView
      identifier={chip.title}
      title={issue?.title ?? NODE_UNSYNCED_TITLE}
      status={nodeDisplayGlyph(chip.display)}
    />
  )
  return chip.stacked ? <IssueChipStack>{view}</IssueChipStack> : view
}

function NodeStrip({
  strip,
  nodeById,
  issueById,
  teamId,
  selection,
  onSelect,
  onMenu,
}: {
  strip: { wave: number; nodes: NodeChip[] }[]
  nodeById: ReadonlyMap<string, WorkflowNode>
  issueById: ReadonlyMap<string, Issue>
  teamId: string
  selection: StripSelection
  onSelect: (id: string | null, modifiers: { toggle: boolean; extend: boolean }) => void
  onMenu: (nodeId: string, item: NodeChipMenuItem) => void
}) {
  const picked = new Set(selection.ids)
  return (
    <div
      className="-mx-4 flex items-start gap-4 overflow-x-auto px-4 pt-1.5 pb-1"
      data-testid="workflow-strip"
    >
      <Button
        size="xs"
        variant={picked.size === 0 ? `secondary` : `ghost`}
        aria-pressed={picked.size === 0}
        data-testid="workflow-node-all"
        className="shrink-0"
        onClick={() => onSelect(null, { toggle: false, extend: false })}
      >
        {ALL_NODES_LABEL}
      </Button>
      {strip.map((wave) => (
        <div
          key={wave.wave}
          className="flex shrink-0 flex-col items-start gap-2"
          data-testid={`workflow-wave-${wave.wave}`}
        >
          {wave.nodes.map((chip) => {
            const node = nodeById.get(chip.id)
            const issue = node ? issueById.get(node.issueId) : undefined
            const menu = node ? nodeChipMenu(node.state) : []
            const selected = picked.has(chip.id)
            const trigger = (
              <button
                type="button"
                aria-pressed={selected}
                aria-label={`${chip.title} ${chip.caption}`}
                data-testid={`workflow-node-${chip.id}`}
                data-display={chip.display}
                className={cn(
                  `inline-flex max-w-[18rem] min-w-0 items-center gap-1.5 rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
                  selected && `ring-1 ring-primary`
                )}
                onClick={(event) => {
                  // Keeps the mini-graph popover (the trigger's own toggle)
                  // out of a pick: hover opens it, a click selects.
                  event.preventDefault()
                  onSelect(chip.id, {
                    toggle: event.metaKey || event.ctrlKey,
                    extend: event.shiftKey,
                  })
                }}
              >
                <NodeChipView chip={chip} issue={issue} />
                {chip.live && (
                  <LiveDot
                    tone="live"
                    ping
                    className="shrink-0"
                    label="Working"
                  />
                )}
                {chip.needsYou && (
                  <LiveDot
                    tone="attention"
                    className="shrink-0"
                    label={NEEDS_YOU_LABEL}
                  />
                )}
              </button>
            )
            return (
              <div key={chip.id} className="flex items-center gap-0.5">
                {node ? (
                  <IssueBlocksPopover
                    issueId={node.issueId}
                    teamId={teamId}
                    label={chip.title}
                    trigger={trigger}
                  />
                ) : (
                  trigger
                )}
                {menu.length > 0 && (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-6"
                        aria-label={`${chip.title} actions`}
                        data-testid={`workflow-node-${chip.id}-menu`}
                      >
                        <MoreIcon className="size-3.5" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="start">
                      {menu.map((item) => (
                        <DropdownMenuItem
                          key={item}
                          variant={
                            item === `skip` || item === `dismiss`
                              ? `destructive`
                              : `default`
                          }
                          data-testid={`workflow-node-${chip.id}-${item}`}
                          onSelect={() => onMenu(chip.id, item)}
                        >
                          {NODE_MENU_LABEL[item]}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                )}
              </div>
            )
          })}
        </div>
      ))}
    </div>
  )
}

// ── Open questions ──────────────────────────────────────────────────────────

/** At the TOP while a node's run asks the person something: the node, the
 *  question and the run's own steer composer — an answer IS a message to the
 *  run (the same path its Run face sends on). A teammate's run cannot be
 *  steered from here (EXP-312), so it shows the question alone. */
function OpenQuestionsBanner({
  questions,
  sessionById,
  nodeById,
  issueById,
  currentUserId,
  teamId,
  onPick,
}: {
  questions: readonly WorkflowOpenQuestion[]
  sessionById: ReadonlyMap<string, CodingSession>
  nodeById: ReadonlyMap<string, WorkflowNode>
  issueById: ReadonlyMap<string, Issue>
  currentUserId: string | undefined
  teamId: string
  onPick: (nodeId: string) => void
}) {
  const { users } = useTeamUsers(teamId)
  return (
    <div className="flex flex-col gap-2" data-testid="workflow-questions">
      {questions.map((question) => {
        const node = nodeById.get(question.nodeId)
        const issue = node ? issueById.get(node.issueId) : undefined
        const session = sessionById.get(question.sessionId)
        const mine = Boolean(session && currentUserId && session.userId === currentUserId)
        return (
          <Alert
            key={`${question.sessionId}-${question.askedAt}`}
            variant="destructive"
            data-testid={`workflow-question-${question.nodeId}`}
          >
            <AlertDescription className="w-full">
              <div className="flex min-w-0 items-center gap-2">
                {issue && (
                  <IssueChipView
                    identifier={issue.identifier}
                    title={issue.title}
                    status={nodeDisplayGlyph(
                      workflowNodeDisplayState(node?.state ?? ``)
                    )}
                    onClick={() => onPick(question.nodeId)}
                  />
                )}
                <span className="shrink-0 text-xs">{NEEDS_YOU_LABEL}</span>
              </div>
              <p className="whitespace-pre-wrap text-foreground">{question.question}</p>
              {mine && session && (
                <div className="w-full">
                  <QuestionComposer session={session} users={users} />
                </div>
              )}
            </AlertDescription>
          </Alert>
        )
      })}
    </div>
  )
}

function QuestionComposer({
  session,
  users,
}: {
  session: CodingSession
  users: Parameters<typeof SteerComposer>[0][`users`]
}) {
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot)
  useEffect(() => store.connect(), [store])
  return (
    <SteerComposer
      store={store}
      live={snapshot.phase.kind === `live`}
      onSend={(text) => store.sendMessage(text)}
      working={false}
      sessionId={session.id}
      agent={session.agent}
      config={snapshot.config}
      users={users}
    />
  )
}

// ── Faces ───────────────────────────────────────────────────────────────────

/** One node × Issue: the Work screen's Issue face for the node's issue. */
function NodeIssueFace({
  issue,
  teamSlug,
}: {
  issue: Issue | undefined
  teamSlug: string
}) {
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  const { users } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team)
  const board = issue
    ? (boards as Board[]).find((row) => row.id === issue.boardId)
    : undefined
  if (!issue || !board || !team) {
    return <p className="px-4 py-3 text-sm text-muted-foreground">{NODE_UNSYNCED_TITLE}</p>
  }
  return (
    <div className="min-h-0 flex-1">
      <IssueDetailView
        issue={issue}
        users={users}
        board={board}
        teamSlug={teamSlug}
        teamId={team.id}
        readOnly={!permissions.canMutateIssue(issue)}
        showMobileHeader={false}
      />
    </div>
  )
}

/** All (or several nodes) × Issue: the covered issues as the board's own
 *  nested list — sub-issues under their compound parent. A row that IS a
 *  node picks it; a sub-issue opens its page. */
function WorkflowIssueList({
  teamSlug,
  teamId,
  issueIds,
  issueById,
  relations,
  graph,
  nodes,
  onPickNode,
}: {
  teamSlug: string
  teamId: string
  issueIds: readonly string[]
  issueById: ReadonlyMap<string, Issue>
  relations: ReturnType<typeof useTeamIssueGraph>[`relations`]
  graph: ReturnType<typeof useTeamIssueGraph>
  nodes: readonly WorkflowNode[]
  onPickNode: (nodeId: string) => void
}) {
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const boards = useTeamBoards(teamId)
  const labels = useTeamLabels(teamId)
  const { users, userMap } = useTeamUsers(teamId)
  const { options, resolve } = useTeamStatuses(teamId)
  const { data: issueLabelRows } = useLiveQuery(
    (query) =>
      query
        .from({ il: issueLabelCollection })
        .where(({ il }) => eq(il.teamId, teamId)),
    [teamId]
  )
  const issues = useMemo(
    () =>
      issueIds
        .map((id) => issueById.get(id))
        .filter((issue): issue is Issue => Boolean(issue)),
    [issueIds, issueById]
  )
  const groups = useMemo(
    () => nestIssueGroups(buildVisibleIssueGroups(issues, options, resolve), relations),
    [issues, options, resolve, relations]
  )
  const issueLabelMap = useMemo(
    () => buildIssueLabelMap((issueLabelRows ?? []) as IssueLabel[], labels),
    [issueLabelRows, labels]
  )
  const nodeByIssue = useMemo(
    () => new Map(nodes.map((node) => [node.issueId, node.id])),
    [nodes]
  )
  return (
    <IssueList
      groups={groups}
      issueLabelMap={issueLabelMap}
      users={users}
      userMap={userMap}
      issueGraph={graph}
      graphTeamId={teamId}
      onNewIssue={() => {}}
      canCreate={false}
      canMutateIssue={permissions.canMutateIssue}
      canModerate={permissions.isModerator}
      onIssueClick={(issue) => {
        const nodeId = nodeByIssue.get(issue.id)
        if (nodeId) {
          onPickNode(nodeId)
          return
        }
        const board = (boards as Board[]).find((row) => row.id === issue.boardId)
        if (!board) return
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
          params: { teamSlug, boardSlug: board.slug, issueIdentifier: issue.identifier },
        })
      }}
    />
  )
}

/** The workflow's dated answers + the engine's event log, folded by default. */
function DecisionsLog({ workflow }: { workflow: SyncedWorkflow }) {
  const [open, setOpen] = useState(false)
  const { data: eventRows } = useLiveQuery(
    (query) =>
      query
        .from({ e: workflowEventCollection })
        .where(({ e }) => eq(e.workflowId, workflow.id)),
    [workflow.id]
  )
  const events = (eventRows ?? []) as WorkflowEvent[]
  const decisions = workflow.decisions.trim()
  if (!decisions && events.length === 0) return null
  return (
    <div className="flex flex-col gap-2 px-4 py-3" data-testid="workflow-decisions">
      <DisclosureHeader open={open} onToggle={() => setOpen(!open)} className="text-xs">
        {DECISIONS_LABEL}
      </DisclosureHeader>
      {open && (
        <>
          {decisions && (
            <p className="whitespace-pre-wrap text-xs text-muted-foreground">{decisions}</p>
          )}
          <WorkflowEventList
            events={events.map((event) => ({
              id: event.id,
              kind: event.kind,
              message: event.message,
              at: event.at,
              nodeId: event.nodeId,
              sessionId: event.sessionId,
            }))}
          />
        </>
      )}
    </div>
  )
}

/** × Runs: the session tree over the scoped runs; a row opens the run IN
 *  PLACE below it (the steer view). One picked node opens its run at once. */
function RunsFace({
  teamId,
  teamSlug,
  sessions,
  defaultRunId,
  currentUserId,
}: {
  teamId: string
  teamSlug: string
  sessions: readonly CodingSession[]
  defaultRunId: string | null
  currentUserId: string | undefined
}) {
  const rows = useSessionListRows(teamId, sessions)
  const [openRunId, setOpenRunId] = useState<string | null>(defaultRunId)
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        className={cn(
          `overflow-y-auto`,
          openRunId ? `max-h-48 shrink-0 border-b border-border` : `min-h-0 flex-1`
        )}
      >
        <SessionTree
          rows={rows}
          teamId={teamId}
          teamSlug={teamSlug}
          activeSessionId={openRunId}
          onOpen={(session) => setOpenRunId(session.id)}
        />
      </div>
      {openRunId && (
        <EmbeddedRun
          key={openRunId}
          teamId={teamId}
          sessionId={openRunId}
          currentUserId={currentUserId}
          onClose={() => setOpenRunId(null)}
        />
      )}
    </div>
  )
}

/** The run's own steer view, embedded. A teammate's run is theirs alone
 *  (EXP-312): its status badge, no view, no ticket. */
function EmbeddedRun({
  teamId,
  sessionId,
  currentUserId,
  onClose,
}: {
  teamId: string
  sessionId: string
  currentUserId: string | undefined
  onClose: () => void
}) {
  const { row, session } = useSessionRow(teamId, currentUserId, sessionId)
  const [face, setFace] = useState<WorkFace>(`run`)
  if (!row || !session || !currentUserId) return null
  if (session.userId !== currentUserId) {
    return (
      <div className="flex items-center gap-2 px-4 py-3" data-testid="workflow-run-foreign">
        <SessionStatusBadge session={session} prState={rowPrState(session, row.issue)} />
        <span className="text-sm text-muted-foreground">
          Only the owner can steer this session.
        </span>
      </div>
    )
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="workflow-run">
      <AgentSessionView
        session={session}
        currentUserId={currentUserId}
        identity={sessionIdentity(row)}
        mergeTarget={row.mergeTarget}
        face={face}
        onFace={(next) => {
          if (next !== `issue`) setFace(next)
        }}
        onBack={onClose}
      />
    </div>
  )
}

/** × Changes for one node: its pull request's files (or its pushed branch). */
function NodeChanges({
  node,
  issue,
  single,
}: {
  node: WorkflowNode
  issue: Issue | undefined
  single: boolean
}) {
  const { state } = useReviewFiles(issue ?? null, { enabled: Boolean(issue) })
  const [selected, setSelected] = useState<string | null>(null)
  if (!issue) return null
  if (!single && state.kind !== `files`) return null
  return (
    <section className="flex flex-col gap-2 py-2" data-testid={`workflow-changes-${node.id}`}>
      {!single && (
        <div className="flex">
          <IssueChipView
            identifier={issue.identifier}
            title={issue.title}
            status={nodeDisplayGlyph(workflowNodeDisplayState(node.state))}
          />
        </div>
      )}
      {state.kind === `error` ? (
        <p className="text-sm text-destructive">{state.message}</p>
      ) : state.kind === `loading` ? null : (
        <ChangesView
          files={state.kind === `files` ? state.files : []}
          nav={single ? `auto` : `none`}
          selected={selected}
          onSelect={setSelected}
          emptyLabel="No changes yet."
        />
      )}
    </section>
  )
}

/** × Results: every scoped run's published screenshots, by topic. */
function ResultsFace({ sessions }: { sessions: readonly CodingSession[] }) {
  const results = useMemo(
    () => sessions.flatMap((session) => parseSessionResults(session.results)),
    [sessions]
  )
  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-4">
      {results.length === 0 ? (
        <p className="py-3 text-sm text-muted-foreground">No results yet.</p>
      ) : (
        <SessionResultsView
          results={results}
          attachmentSrc={(id) => `/api/attachments/${id}`}
        />
      )}
    </div>
  )
}
