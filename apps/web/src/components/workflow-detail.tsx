import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
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
  Skeleton,
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
  isLetterStepKey,
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
import { useOpenComposer } from "@/hooks/use-open-composer"
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
import { BUILTIN_PLAN_WORKFLOW_ID } from "@/lib/builtin-actions"
import { sessionIdentity } from "@/lib/session-identity"
import { acquireSteerSession } from "@/lib/steer-session-store"
import { deviceIsOnline } from "@/lib/steer-devices"
import { OPEN_FINAL_PR_LABEL } from "@/lib/workflow-final-pr-identity"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { cn } from "@/lib/utils"
import {
  workflowOpenQuestions,
  type WorkflowOpenQuestion,
} from "@/lib/workflows/open-questions"
import {
  ADMIT_NODE_LABEL,
  ALL_NODES_LABEL,
  CANCEL_WORKFLOW_CONFIRM,
  DECISIONS_LABEL,
  PICK_DEVICE_LABEL,
  PLAN_WORKFLOW_LABEL,
  REVIEW_FINAL_PR_LABEL,
  NO_CHANGES_LABEL,
  NO_RESULTS_LABEL,
  NO_RUNS_LABEL,
  DISMISS_NODE_CONFIRM,
  RUNS_ON_LABEL,
  STOP_WORKFLOW_LABEL,
  workflowOverflowMenu,
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
  workflowHeaderCaption,
  workflowNodeDisplayState,
  workflowNodeStrip,
  workflowPrimaryAction,
  workflowStartBlocker,
  workflowStatusGlyph,
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
const PauseIcon = conceptIcon(`run-pause`)



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

/** Where a DOCUMENT-level j/k must not step: typing, a dialog or menu, the
 *  face toggle's tabs (Radix owns their keys), and the body (the diff, the
 *  transcript and the issue have their own keys and focusables). The strip
 *  handles its own keys, so it is skipped here too. */
function isKeyOwningTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return (
    target.closest(
      `input, textarea, select, [contenteditable="true"], [role="dialog"], [role="menu"], [role="tab"], [role="tablist"], [data-workflow-body], [data-testid="workflow-strip"]`
    ) !== null
  )
}

/** A text field of the page (the name, the embedded description or comment
 *  editor, the question composer): while one holds focus a chip's hover
 *  must not open the mini-graph, whose popover would steal the focus. */
export function isTextFieldTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return (
    target.closest(
      `input, textarea, [contenteditable="true"], .ProseMirror`
    ) !== null
  )
}

/** A keydown the STRIP must leave alone: typing, or inside a menu, a dialog
 *  or a popover (a chip's `…` menu, the mini-graph) — portaled, but their
 *  events still bubble through the React tree to the strip. */
export function isStripForeignTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  return (
    target.closest(
      `input, textarea, select, [contenteditable="true"], [role="menu"], [role="dialog"], [data-radix-popper-content-wrapper]`
    ) !== null
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
  // Only a RESOLVED device names the runner: while devices load (or for a
  // device the viewer cannot see) the caption drops its `on …` part rather
  // than print a raw uuid. The primary action keys on the id's presence.
  const deviceLabel = runner?.deviceLabel || null
  // EXP-1102: a live workflow's runner went offline, or no row names its id
  // any more (the machine re-minted its identity): the page says so and
  // offers to re-bind to an online machine — the person picks, never an
  // automatic failover; the new host rebuilds its state from the rows.
  const runnerOffline =
    (workflow.status === `running` || workflow.status === `paused`) &&
    workflow.deviceId !== null &&
    remote.devices !== null &&
    (!runner || runner.online === false)

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
        // The strip only orders; the edges are the mini-graph popover's.
      ),
    [nodes, sessionById, issueById, questions]
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
  const [openingFinalPr, setOpeningFinalPr] = useState(false)
  const [runsOnOpenState, setRunsOnOpen] = useState(false)
  // While the name is being edited a chip's hover must not open the
  // mini-graph: the popover takes focus and the blur would save the name.
  const [nameEditing, setNameEditing] = useState(false)
  // Any text field of the page focused (focusin/focusout on the root, the
  // React tree's portals included): the same hover gate for every editor.
  const [textEditing, setTextEditing] = useState(false)
  const runsOnRequested = useRef(false)
  // The runner is frozen once the workflow started: the picker can never
  // write `deviceId` on a running workflow, even if it was open at the flip.
  const isDraft = workflow.status === `draft`
  // EXP-1102: … unless the runner is gone, when the pick is offered again.
  const canPickRunner = isDraft || runnerOffline
  const runsOnOpen = runsOnOpenState && canPickRunner
  useEffect(() => {
    if (!canPickRunner) setRunsOnOpen(false)
  }, [canPickRunner])
  const openComposer = useOpenComposer()
  const [dialog, setDialog] = useState<
    | `cancel`
    | `delete`
    | `merge`
    | { skip: string }
    | { dismiss: string }
    | null
  >(null)

  // A node a replan folded away must not stay picked. Only once the nodes
  // synced: an initial `?node=` would otherwise be pruned on the first frame.
  useEffect(() => {
    if (order.length > 0) setSelection((current) => pruneSelection(current, order))
  }, [order])

  // The pick + face live in the URL (`?node=` for a single pick, `?face=`
  // off Issue), so a link to a node × face is shareable. `replace`: stepping
  // through the strip must not spam history. Skips the unchanged first run.
  const urlNode = selection.ids.length === 1 ? selection.ids[0] : undefined
  const urlFace = face === `issue` ? undefined : face
  const written = useRef(
    `${initialNodeId ?? ``}|${initialFace === `issue` ? `` : initialFace}`
  )
  // An EXTERNAL navigation (the session tree's workflow row = the bare URL,
  // a `?node=`/`?face=` link) re-seeds the pick + face. The page's own
  // write-back lands here too, but its key equals `written`: no loop.
  useEffect(() => {
    const searchFace = initialFace === `issue` ? `` : initialFace
    const key = `${initialNodeId ?? ``}|${searchFace}`
    if (written.current === key) return
    written.current = key
    setSelection(
      initialNodeId
        ? { ids: [initialNodeId], anchor: initialNodeId, cursor: initialNodeId }
        : ALL_SELECTION
    )
    setFace(initialFace)
  }, [initialNodeId, initialFace])
  useEffect(() => {
    const key = `${urlNode ?? ``}|${urlFace ?? ``}`
    if (written.current === key) return
    written.current = key
    void navigate({
      to: `/t/$teamSlug/workflows/$workflowId`,
      params: { teamSlug, workflowId: workflow.id },
      search: { node: urlNode, face: urlFace },
      replace: true,
    })
  }, [urlNode, urlFace, navigate, teamSlug, workflow.id])

  // j/k step from anywhere that does not own its keys (letters never scroll).
  // ←/→ are the STRIP's own (`onStripKeyDown`): at document level they would
  // steal arrow scrolling from the diff and the transcript, and fight the
  // face toggle's Radix tabs.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isLetterStepKey(event.key)) return
      if (isKeyOwningTarget(event.target)) return
      const direction = stripStepKey(event)
      if (direction === null) return
      event.preventDefault()
      setSelection((current) => stepSelection(current, direction, order))
    }
    document.addEventListener(`keydown`, onKey)
    return () => document.removeEventListener(`keydown`, onKey)
  }, [order])

  /** ←/→ and j/k while focus is inside the strip; an unconsumed key is left
   *  alone (no preventDefault), so Tab, Enter and scrolling keep working. */
  const onStripKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.defaultPrevented) return
    // A chip's portaled menu and the mini-graph popover bubble their keys
    // here through the React tree: they own them.
    if (isStripForeignTarget(event.target)) return
    const direction = stripStepKey(event)
    if (direction === null) return
    event.preventDefault()
    setSelection((current) => stepSelection(current, direction, order))
  }

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
    if (item === `dismiss`) {
      setDialog({ dismiss: nodeId })
      return
    }
    if (item === `retry`) {
      void run(`The node could not be retried`, () =>
        trpc.workflows.resolveNode.mutate({ nodeId, action: `retry` }, quiet)
      )
      return
    }
    void run(`The node could not be admitted`, () =>
      trpc.workflows.admitNode.mutate({ nodeId, admit: true }, quiet)
    )
  }

  // EXP-1059: a final PR closed WITHOUT merging: the member's way back.
  // `workflows.openFinalPr` reopens it, or opens a fresh one when GitHub
  // refuses; the synced row swaps the row's action back to Merge. Only a
  // live workflow (running or paused) offers it: the server refuses the
  // reopen for a cancelled or done one, so the row shows nothing there.
  const openFinalPr = async () => {
    setError(null)
    setOpeningFinalPr(true)
    try {
      await trpc.workflows.openFinalPr.mutate(
        { id: workflow.id },
        { context: { skipErrorToast: true } }
      )
    } catch (caught) {
      setError(
        trpcErrorMessage(caught, `The final pull request could not be opened`)
      )
    } finally {
      setOpeningFinalPr(false)
    }
  }

  const primary = workflowPrimaryAction(
    workflow.status,
    workflow.deviceId,
    typeof workflow.finalPrState === `string` ? workflow.finalPrState : null
  )
  const cycleNote = workflowCycleNote(workflow.metrics)
  const startBlocker =
    workflow.status === `draft` && primary === `start`
      ? workflowStartBlocker(workflow, workflow.metrics)
      : null
  const notice = workflow.status === `draft` ? (cycleNote ?? startBlocker) : null

  const runnerDevices: DevicePickerDevice[] = (remote.devices ?? [])
    .filter(
      (device) =>
        device.deviceId === workflow.deviceId ||
        (deviceIsOnline(device) && (device.caps ?? []).includes(`workflows`))
    )
    .map((device) => ({
      id: device.deviceId,
      name: `${device.deviceLabel || device.deviceId}${
        device.owner ? ` · ${device.owner.name}` : ``
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
    dialog && typeof dialog === `object` && `skip` in dialog ? dialog.skip : null
  const dismissNodeId =
    dialog && typeof dialog === `object` && `dismiss` in dialog
      ? dialog.dismiss
      : null

  return (
    <div
      className="flex h-full min-h-0 flex-col"
      data-testid="workflow-detail"
      onFocus={(event) => setTextEditing(isTextFieldTarget(event.target))}
      onBlur={() => setTextEditing(false)}
    >
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
              {...nodeDisplayGlyph(workflowStatusGlyph(workflow.status))}
              className="size-4 shrink-0"
            />
            <WorkflowNameField
              name={workflow.name}
              onRename={(name) => void save({ name })}
              onEditingChange={setNameEditing}
            />
            <PrimaryAction
              action={primary}
              startDisabled={startBlocker !== null}
              devices={runnerDevices}
              deviceId={workflow.deviceId}
              onPickDevice={(deviceId) => void save({ deviceId })}
              onIntent={(which) => void intent(which)}
              onReview={() => {
                // The final PR lives on this page (All × Changes: its link,
                // and Merge while open); the Reviews queue's Workflows group
                // (EXP-1072) points back here for the same row.
                setSelection(ALL_SELECTION)
                setFace(`changes`)
              }}
            />
            <span className="relative inline-flex shrink-0">
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
                <DropdownMenuContent
                  align="end"
                  onCloseAutoFocus={(event) => {
                    // Runs on hands focus to its picker, not back to `…`
                    // (the refocus would dismiss the just-opened popover).
                    if (runsOnRequested.current) event.preventDefault()
                    runsOnRequested.current = false
                  }}
                >
                  {workflowOverflowMenu(workflow.status).map((item) =>
                    item === `plan` ? (
                      <DropdownMenuItem
                        key={item}
                        data-testid="workflow-plan"
                        onSelect={() =>
                          openComposer({
                            actionId: BUILTIN_PLAN_WORKFLOW_ID,
                            workflowId: workflow.id,
                          })
                        }
                      >
                        {PLAN_WORKFLOW_LABEL}
                      </DropdownMenuItem>
                    ) : item === `runs_on` ? (
                      <DropdownMenuItem
                        key={item}
                        data-testid="workflow-runs-on"
                        onSelect={() => {
                          runsOnRequested.current = true
                          setRunsOnOpen(true)
                        }}
                      >
                        {deviceLabel
                          ? `${RUNS_ON_LABEL} · ${deviceLabel}`
                          : RUNS_ON_LABEL}
                      </DropdownMenuItem>
                    ) : item === `stop` ? (
                      <DropdownMenuItem
                        key={item}
                        variant="destructive"
                        data-testid="workflow-cancel"
                        onSelect={() => setDialog(`cancel`)}
                      >
                        {STOP_WORKFLOW_LABEL}
                      </DropdownMenuItem>
                    ) : (
                      <DropdownMenuItem
                        key={item}
                        variant="destructive"
                        data-testid="workflow-delete"
                        onSelect={() => setDialog(`delete`)}
                      >
                        {DELETE_WORKFLOW_LABEL}
                      </DropdownMenuItem>
                    )
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
              {runsOnOpen && (
                <DevicePicker
                  mobileTitle={RUNS_ON_LABEL}
                  value={workflow.deviceId}
                  devices={runnerDevices}
                  open
                  onOpenChange={setRunsOnOpen}
                  align="end"
                  // The overflow item opens it; this zero-size span under the
                  // `…` button is only the popover's ANCHOR, so the picker
                  // hangs off the overflow on a pointer device (the phone
                  // renders the sheet and ignores it).
                  trigger={
                    <span
                      aria-hidden
                      tabIndex={-1}
                      className="pointer-events-none absolute right-0 bottom-0 size-0"
                      data-testid="workflow-runs-on-anchor"
                    />
                  }
                  data-testid="workflow-runs-on-picker"
                  onChange={(deviceId) => {
                    setRunsOnOpen(false)
                    if (canPickRunner) void save({ deviceId })
                  }}
                />
              )}
            </span>
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
          {runnerOffline && (
            <p className="pl-7 text-xs text-destructive" data-testid="workflow-runner-offline">
              {runnerOfflineCaption(runner?.lastSeenAt ?? null)}{` `}
              <Button
                variant="link"
                size="inline"
                className="text-destructive underline underline-offset-2"
                data-testid="workflow-runner-rebind"
                onClick={() => {
                  runsOnRequested.current = true
                  setRunsOnOpen(true)
                }}
              >
                {REBIND_RUNNER_LABEL}
              </Button>
            </p>
          )}
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
          onKeyDown={onStripKeyDown}
          hoverOpensGraph={!nameEditing && !textEditing}
          onSelect={(id, modifiers) =>
            setSelection((current) => selectNode(current, id, order, modifiers))
          }
          onMenu={nodeAction}
        />

        <WorkFaceToggle face={toggleFace} items={faceItems} />
      </div>

      <div
        className="flex min-h-0 flex-1 flex-col"
        data-workflow-body=""
        data-testid={`workflow-body-${body}`}
      >
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
                {workflow.finalPrState === `closed` &&
                  (workflow.status === `running` ||
                    workflow.status === `paused`) && (
                  <Button
                    size="sm"
                    disabled={openingFinalPr}
                    data-testid="workflow-final-pr-open"
                    onClick={() => void openFinalPr()}
                  >
                    {OPEN_FINAL_PR_LABEL}
                  </Button>
                )}
              </div>
            )}
            {scopeNodes.length === 0 &&
              !(picked.length === 0 && workflow.finalPrUrl) && (
                <p
                  className="py-3 text-sm text-muted-foreground"
                  data-testid="workflow-changes-empty"
                >
                  {NO_CHANGES_LABEL}
                </p>
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
        title={STOP_WORKFLOW_LABEL}
        description={CANCEL_WORKFLOW_CONFIRM}
        confirm={STOP_WORKFLOW_LABEL}
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
      <ConfirmDialog
        open={dismissNodeId !== null}
        onClose={() => setDialog(null)}
        title={DISMISS_NODE_LABEL}
        description={DISMISS_NODE_CONFIRM}
        confirm={DISMISS_NODE_LABEL}
        destructive
        testId="workflow-node-dismiss-confirm"
        onConfirm={() => {
          setDialog(null)
          if (dismissNodeId) {
            void run(`The node could not be dismissed`, () =>
              trpc.workflows.admitNode.mutate(
                { nodeId: dismissNodeId, admit: false },
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
  onEditingChange,
}: {
  name: string
  onRename: (name: string) => void
  onEditingChange: (editing: boolean) => void
}) {
  const [draft, setDraft] = useState(name)
  const [editing, setEditing] = useState(false)
  // Escape = discard: the blur it triggers reads this and never saves (the
  // blur handler's closure still holds the edited draft).
  const discard = useRef(false)
  useEffect(() => {
    if (!editing) setDraft(name)
  }, [name, editing])
  return (
    <Input
      value={draft}
      aria-label="Workflow name"
      data-testid="workflow-name"
      className="h-8 min-w-0 flex-1 border-0 bg-transparent px-1 text-base font-semibold shadow-none focus-visible:ring-0"
      onFocus={() => {
        discard.current = false
        setEditing(true)
        onEditingChange(true)
      }}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => {
        setEditing(false)
        onEditingChange(false)
        const next = draft.trim()
        if (!discard.current && next && next !== name) onRename(next)
        else setDraft(name)
        discard.current = false
      }}
      onKeyDown={(event) => {
        if (event.key === `Enter`) event.currentTarget.blur()
        if (event.key === `Escape`) {
          discard.current = true
          setDraft(name)
          event.currentTarget.blur()
        }
      }}
    />
  )
}

/** EXP-1102: the re-bind offer beside the offline caption. */
export const REBIND_RUNNER_LABEL = `Pick another device`

/** EXP-1102: what the header says about a runner that is not beating any
 *  more — "Runner offline since 14:49", or without a stamp when no row names
 *  the bound id at all (the machine re-minted its identity). */
export function runnerOfflineCaption(lastSeenAt: string | null): string {
  if (!lastSeenAt) return `Runner offline.`
  const seen = new Date(lastSeenAt)
  if (Number.isNaN(seen.getTime())) return `Runner offline.`
  const hh = String(seen.getHours()).padStart(2, `0`)
  const mm = String(seen.getMinutes()).padStart(2, `0`)
  return `Runner offline since ${hh}:${mm}.`
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
            mobileTitle={PICK_DEVICE_LABEL}
            value={deviceId}
            devices={devices}
            onChange={onPickDevice}
            trigger={<Button size="sm">{PICK_DEVICE_LABEL}</Button>}
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
          <PauseIcon className="size-4" />
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
          {REVIEW_FINAL_PR_LABEL}
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
  onKeyDown,
  hoverOpensGraph,
  onSelect,
  onMenu,
}: {
  strip: { wave: number; nodes: NodeChip[] }[]
  nodeById: ReadonlyMap<string, WorkflowNode>
  issueById: ReadonlyMap<string, Issue>
  teamId: string
  selection: StripSelection
  onKeyDown: (event: ReactKeyboardEvent<HTMLElement>) => void
  /** False while a text field of the page is being edited. */
  hoverOpensGraph: boolean
  onSelect: (id: string | null, modifiers: { toggle: boolean; extend: boolean }) => void
  onMenu: (nodeId: string, item: NodeChipMenuItem) => void
}) {
  const picked = new Set(selection.ids)
  // Roving tabindex: ONE chip of the strip is in the tab order — the one the
  // keyboard steps from (the cursor, else the last pick, else `All`).
  const current =
    selection.ids.length === 0
      ? null
      : selection.cursor && picked.has(selection.cursor)
        ? selection.cursor
        : selection.ids[selection.ids.length - 1]!
  const ref = useRef<HTMLDivElement>(null)
  // Focus follows a keyboard step while focus is in the strip.
  useEffect(() => {
    const root = ref.current
    if (!root || !root.contains(document.activeElement)) return
    const target = root.querySelector<HTMLElement>(
      `[data-testid="workflow-node-${current ?? `all`}"]`
    )
    if (target && target !== document.activeElement) target.focus()
  }, [current])
  return (
    <div
      ref={ref}
      role="toolbar"
      aria-label="Nodes"
      className="-mx-4 flex items-start gap-4 overflow-x-auto px-4 pt-1.5 pb-1"
      data-testid="workflow-strip"
      onKeyDown={onKeyDown}
    >
      <Button
        size="xs"
        variant={picked.size === 0 ? `secondary` : `ghost`}
        aria-pressed={picked.size === 0}
        tabIndex={current === null ? 0 : -1}
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
              <Button
                variant="ghost"
                size="inline"
                aria-pressed={selected}
                aria-label={`${chip.title} ${chip.caption}`}
                title={chip.caption}
                tabIndex={current === chip.id ? 0 : -1}
                data-testid={`workflow-node-${chip.id}`}
                data-display={chip.display}
                className={cn(
                  `max-w-[18rem] min-w-0 gap-1.5 rounded-md text-sm font-normal hover:bg-transparent aria-pressed:bg-transparent dark:hover:bg-transparent`,
                  selected && `ring-1 ring-primary`
                )}
                onClick={(event) => {
                  const modifiers = {
                    toggle: event.metaKey || event.ctrlKey,
                    extend: event.shiftKey,
                  }
                  // The chip is BOTH the pick and the mini-graph popover's
                  // trigger (`IssueBlocksPopover` opens on hover on a pointer
                  // device and toggles on click everywhere). A click that
                  // CHANGES the pick keeps the popover shut (preventDefault
                  // skips Radix's toggle); a plain click on the chip that is
                  // already the one pick lets the toggle through. So on a
                  // phone (no hover) the first tap selects and a second tap
                  // on the selected chip opens the mini-graph.
                  const alreadyPicked =
                    selected && selection.ids.length === 1
                  if (alreadyPicked && !modifiers.toggle && !modifiers.extend) {
                    return
                  }
                  event.preventDefault()
                  onSelect(chip.id, modifiers)
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
              </Button>
            )
            return (
              <div key={chip.id} className="flex items-center gap-0.5">
                {node ? (
                  <IssueBlocksPopover
                    issueId={node.issueId}
                    teamId={teamId}
                    label={chip.title}
                    trigger={trigger}
                    openOnHover={hoverOpensGraph}
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
          emptyNote={NO_RUNS_LABEL}
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

/** × Changes for one node: its pull request's files (or its pushed branch).
 *  With several nodes in scope EVERY node keeps its row (chip + state): a
 *  node with nothing to show reads `No changes yet`, a loading one a
 *  skeleton. */
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
  const files = state.kind === `files` ? state.files : []
  const empty = !issue || state.kind === `none` || (state.kind === `files` && files.length === 0)
  return (
    <section className="flex flex-col gap-2 py-2" data-testid={`workflow-changes-${node.id}`}>
      {!single && issue && (
        <div className="flex">
          <IssueChipView
            identifier={issue.identifier}
            title={issue.title}
            status={nodeDisplayGlyph(workflowNodeDisplayState(node.state))}
          />
        </div>
      )}
      {state.kind === `error` && issue ? (
        <p className="text-sm text-destructive">{state.message}</p>
      ) : empty ? (
        <p
          className="text-sm text-muted-foreground"
          data-testid={`workflow-changes-${node.id}-empty`}
        >
          {NO_CHANGES_LABEL}
        </p>
      ) : state.kind === `loading` ? (
        <Skeleton
          className="h-16 w-full"
          data-testid={`workflow-changes-${node.id}-loading`}
        />
      ) : (
        <ChangesView
          files={files}
          nav={single ? `auto` : `none`}
          selected={selected}
          onSelect={setSelected}
          emptyLabel={NO_CHANGES_LABEL}
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
        <p
          className="py-3 text-sm text-muted-foreground"
          data-testid="workflow-results-empty"
        >
          {NO_RESULTS_LABEL}
        </p>
      ) : (
        <SessionResultsView
          results={results}
          attachmentSrc={(id) => `/api/attachments/${id}`}
        />
      )}
    </div>
  )
}
