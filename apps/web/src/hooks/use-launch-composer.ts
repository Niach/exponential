import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { toast } from "sonner"
import type { Board, CodingSession, Issue, SyncedDeviceWorktree } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useTeamRepos } from "@/hooks/use-team-repos"
import type { RemoteStart } from "@/hooks/use-remote-start"
import {
  actionCollection,
  codingSessionCollection,
  deviceWorktreeCollection,
  issueCollection,
} from "@/lib/collections"
import {
  BUILTIN_CHAT_ID,
  BUILTIN_CHAT_NAME,
  BUILTIN_CREATE_ACTION_ID,
  builtinCreateAction,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import { buildInputsPayload, missingRequiredInputs } from "@/lib/action-inputs"
import {
  chatRepoOptions,
  chatStartInputs,
  defaultChatRepoId,
  type ChatRepoOption,
} from "@/lib/chat-repo"
import { agentSeed } from "@/lib/coding-launch-prefs"
import { byCreatedAtDesc } from "@/lib/ordering"
import {
  dropPendingImage,
  markPendingImageUploaded,
  stagePendingImages,
  type PendingImage,
} from "@/lib/pending-images"
import { buildSteerImageMessage, MAX_STEER_IMAGES } from "@/lib/steer-image-message"
import {
  deviceAgentLaunchDefaults,
  deviceHasRunnableAgent,
  deviceIsOnline,
  resumeWorktree,
  type SteerDevice,
} from "@/lib/steer-devices"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import {
  useLaunchOptions,
  type LaunchOptions,
} from "@/components/launch-dialog/use-launch-options"
import type { TeamAction } from "@/components/action-editor-dialog"
import type { McpServerList } from "@/lib/mcp-servers"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-825: the ONE launcher's state — the Agent page composer. It replaced
// the three-tab launch dialog (Issues | Actions | Chat, EXP-257) and the
// dedicated create-action dialog (EXP-431): what a run is ABOUT is now a
// SUBJECT picked into the composer (issue chips, or one action chip; picking
// the other kind swaps — no disabled controls), the free text is the chat
// prompt while there is no subject and "additional instructions" once there
// is one, images ride the text as the steer embed format, and the options
// line under the card carries device/agent/model/plan with the rest behind
// a `⋯` popover. Every play button in the app navigates here with a seed
// (`LaunchSeed`, ×4) instead of opening a dialog of its own.
//
// What moved here from the dialog shell verbatim: the codeable-issue pool and
// its running exclusion (EXP-153 staleness), `MAX_ISSUES_PER_RUN`, the
// single-repo guard, the resume worktree offer (EXP-481), the EXP-349
// repo-input seed latch and the EXP-437 device-seeded options cluster.

export type { LaunchSeed }

export type LaunchSubject =
  | { kind: `issues`; ids: string[] }
  | { kind: `action`; id: string; inputs: Record<string, string> }
  | null

// Hard cap per run — parity with the server zod cap (issueIds max 30) and the
// desktop launcher's MAX_ISSUES_PER_RUN. Beyond it the server would reject with
// a zod BAD_REQUEST whose `[`-prefixed message is discarded into a misleading
// "could not be delivered" toast, so block the submit here instead.
export const MAX_ISSUES_PER_RUN = 30
// Above this, batches get a soft token-cost note (matches the native sheets).
export const BATCH_COST_HINT_THRESHOLD = 6

// Only issues in a state worth coding are offered (mirrors the desktop picker).
// EXP-314: deliberately keyed on the dual-written ANCHOR enum, not on status
// rows — a custom backlog/unstarted/started status anchors into this set
// automatically, so custom statuses need no extra gating here.
const CODEABLE_STATUSES = new Set<string>([
  `backlog`,
  `in_progress`,
  `in_review`,
])

// EXP-349: an action bound to a repository pre-fills its repo-typed inputs
// with that repo — the picker showing "None" while the action runs in its
// bound repo anyway read as a misconfiguration. The user can still re-pick
// or clear the field.
function repoInputSeed(action: TeamAction): Record<string, string> {
  if (!action.repositoryId) return {}
  const seed: Record<string, string> = {}
  for (const def of action.inputs) {
    if (def.type === `repo`) seed[def.key] = action.repositoryId
  }
  return seed
}

/** The contract's submit label per subject (×4). */
export function submitLabelFor(subject: LaunchSubject): string {
  if (subject === null) return `Start chat`
  if (subject.kind === `action`) return `Run action`
  return subject.ids.length >= 2
    ? `Start batch · ${subject.ids.length}`
    : `Start coding`
}

export interface LaunchComposerModel {
  teamId: string
  subject: LaunchSubject
  /** The subject action's row (null while it is not synced, or no action). */
  selectedAction: TeamAction | null
  /** Picker list: fix-conflicts pinned first, then Create action, then the
   * team's rows in server order. Null while the shape is loading. */
  actions: TeamAction[] | null
  /** Codeable, not-running issues on repo-backed boards, newest first. */
  eligibleIssues: Issue[]
  /** The checked issues' rows (a checked id not synced yet is absent). */
  checkedIssues: Issue[]
  toggleIssue: (issueId: string) => void
  pickAction: (actionId: string) => void
  clearAction: () => void
  setInput: (key: string, value: string) => void
  /** Any issue linked to the PR the `pr` input opens pre-picked on. */
  seedPrIssueId: string | undefined

  text: string
  setText: (text: string) => void
  images: PendingImage[]
  /** Stages files and drops their markers at `caret`; returns the caret
   * behind the last marker (the field puts its cursor there). */
  addFiles: (files: File[], caret: number) => number
  removeImage: (url: string) => void

  /** The team's connected repositories (null until fetched) — the
   * action inputs' `repo` pickers and the chat picker read it. */
  repos: ChatRepoOption[] | null
  /** Chat only (no subject): the optional repository. */
  repoId: string
  setRepoId: (repoId: string) => void
  repoOptions: { value: string; label: string }[]

  /** EXP-481: the single checked issue's existing worktree on the picked
   * device (with the picked agent), when there is one. */
  resumeCandidate: { identifier: string; branch: string } | null
  resume: boolean
  setResume: (value: boolean) => void
  /** True when the run will resume — plan mode hides behind it. */
  resumeActive: boolean

  launch: LaunchOptions
  /** Online machines with a runnable agent. */
  candidateDevices: SteerDevice[]
  mcpServers: McpServerList | null
  mcpNow: Date

  submitLabel: string
  /** Something stops the submit — the notes say what. */
  blocked: boolean
  overCap: boolean
  spansRepos: boolean
  spansBranches: boolean
  costHint: boolean
  /** Uploading or the start is in flight. */
  busy: boolean
  /** The device label a start was just delivered to (the run opens itself). */
  sentTo: string | null
  submit: () => Promise<void>
}

export function useLaunchComposer({
  teamId,
  remote,
  seed,
  onSeedConsumed,
}: {
  teamId: string
  remote: RemoteStart
  /** The route's one-shot preselection; consumed once, then reported. */
  seed: LaunchSeed | null
  onSeedConsumed: () => void
}): LaunchComposerModel {
  const [subject, setSubject] = useState<LaunchSubject>(null)
  const [text, setText] = useState(``)
  const [images, setImages] = useState<PendingImage[]>([])
  const [repoId, setRepoId] = useState(``)
  // EXP-481: "Resume previous session" — default ON whenever it first becomes
  // eligible (reset when the sole issue changes); a manual toggle sticks.
  const [resume, setResume] = useState(true)
  const [seedPrIssueId, setSeedPrIssueId] = useState<string | undefined>()
  const [sending, setSending] = useState(false)
  // Last action id whose repo inputs were seeded (EXP-349) — the latch keeps
  // a manual re-pick (including clearing to "None") from being re-seeded when
  // the Electric actions rows update.
  const seededRepoActionId = useRef<string | null>(null)
  // A seeded device that is not a candidate YET (the devices shape has not
  // hydrated on a fresh page load) — applied the moment it appears.
  const pendingDeviceRef = useRef<string | null>(null)
  const imagesRef = useRef(images)
  imagesRef.current = images

  // ── Data ──────────────────────────────────────────────────────────────────

  const repos = useTeamRepos(teamId)
  const seededRepo = useRef(false)
  useEffect(() => {
    if (!repos || seededRepo.current) return
    seededRepo.current = true
    setRepoId(defaultChatRepoId(repos))
  }, [repos])
  const repoOptions = useMemo(() => chatRepoOptions(repos ?? []), [repos])

  // Live synced actions (EXP-268 — the body-less list projection); the two
  // listed builtins (not DB rows) pinned first, the rest re-apply the
  // server's ordering (sortOrder, name). Chat is never listed: it IS the
  // no-subject state.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )
  const actions = useMemo<TeamAction[] | null>(() => {
    if (actionRows === undefined) return null
    const rows = [...actionRows]
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((row) => ({ ...row, builtin: false as const }))
    return [
      builtinFixConflictsAction(teamId),
      builtinCreateAction(teamId),
      ...rows,
    ]
  }, [teamId, actionRows])
  const selectedAction =
    subject?.kind === `action`
      ? ((actions ?? []).find((action) => action.id === subject.id) ?? null)
      : null

  // Codeable issues live in boards that HAVE a repo — coding gates on repo
  // presence. Sorted ids keep the dep string stable.
  const boards = useTeamBoards(teamId)
  const repoBoardIds = useMemo(() => {
    const ids = boards.filter((p) => p.repositoryId).map((p) => p.id)
    ids.sort()
    return ids
  }, [boards])
  const { data: issueRows } = useLiveQuery(
    (query) =>
      repoBoardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, repoBoardIds))
        : undefined,
    [repoBoardIds.join(`,`)]
  )
  const { data: runningRows } = useLiveQuery(
    (query) =>
      query.from({ s: codingSessionCollection }).where(({ s }) =>
        and(
          eq(s.teamId, teamId),
          // in_review terminals are still alive and occupy the issue's
          // worktree (EXP-194) — they block a restart like running ones.
          inArray(s.status, [`running`, `in_review`])
        )
      ),
    [teamId]
  )
  // Staleness guard (EXP-153): a heartbeat-dead row must not keep its issue
  // blocked from a fresh start.
  const now = useNow()
  const runningIssueIds = useMemo(() => {
    const set = new Set<string>()
    for (const s of (runningRows ?? []) as CodingSession[]) {
      if (s.issueId && !isCodingSessionStale(s.updatedAt, now))
        set.add(s.issueId)
    }
    return set
  }, [runningRows, now])
  const allById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((i) => [i.id, i])),
    [issueRows]
  )
  const eligibleIssues = useMemo(
    () =>
      ((issueRows ?? []) as Issue[])
        .filter(
          (issue) =>
            CODEABLE_STATUSES.has(issue.status) && !runningIssueIds.has(issue.id)
        )
        .sort(byCreatedAtDesc),
    [issueRows, runningIssueIds]
  )
  const checkedIds = subject?.kind === `issues` ? subject.ids : []
  const checkedIssues = useMemo(
    () =>
      checkedIds
        .map((id) => allById.get(id))
        .filter((i): i is Issue => Boolean(i)),
    [checkedIds, allById]
  )

  // A batch run is ONE repository on ONE base branch (the server enforces
  // both, EXP-712) — resolve each checked issue's board and block a
  // cross-repo / cross-branch selection client-side. A board with no branch
  // override rides the repo's default, which only the server knows, so the
  // branch guard fires only on two DIFFERENT overrides.
  const boardById = useMemo(
    () => new Map(boards.map((board) => [board.id, board as Board])),
    [boards]
  )
  const { spansRepos, spansBranches } = useMemo(() => {
    const repoIds = new Set<string>()
    const branches = new Set<string>()
    for (const issue of checkedIssues) {
      const board = boardById.get(issue.boardId)
      if (board?.repositoryId) repoIds.add(board.repositoryId)
      if (board?.defaultBranch) branches.add(board.defaultBranch)
    }
    return { spansRepos: repoIds.size > 1, spansBranches: branches.size > 1 }
  }, [checkedIssues, boardById])

  // ── Options cluster ───────────────────────────────────────────────────────

  // Device candidates (EXP-639: the whole fleet above the version floor
  // advertises the action/inputs/fix-conflicts caps whenever it advertises a
  // runnable agent, so only these two filters remain — and
  // `steer.startSession` enforces the agent check server-side). EXP-403: the
  // registry lists offline machines too — only online ones are startable.
  const candidateDevices = useMemo(
    () => (remote.devices ?? []).filter(deviceIsOnline).filter(deviceHasRunnableAgent),
    [remote.devices]
  )
  const mcp = useMcpServers(teamId)
  const mcpNow = useNow(30_000)
  const hasSubject = subject !== null
  // EXP-772: a chat (no subject) starts in build mode; a subject seeds plan
  // mode from the device's defaults. `planModeOff` is read on every reseed
  // path inside the hook, and the effect below covers the flip itself.
  const launch = useLaunchOptions({
    open: true,
    devices: candidateDevices,
    initialDeviceId: seed?.deviceId,
    planModeOff: !hasSubject,
    teamId,
    mcpServers: mcp.servers,
  })
  const { device, agent } = launch
  const hadSubjectRef = useRef(hasSubject)
  useEffect(() => {
    if (hadSubjectRef.current === hasSubject) return
    hadSubjectRef.current = hasSubject
    launch.setPlanMode(
      hasSubject
        ? agentSeed(agent, deviceAgentLaunchDefaults(device, agent)).planMode
        : false
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasSubject])

  // A seeded device applies once it is a candidate (see `pendingDeviceRef`).
  useEffect(() => {
    const pending = pendingDeviceRef.current
    if (!pending) return
    if (candidateDevices.some((candidate) => candidate.deviceId === pending)) {
      pendingDeviceRef.current = null
      launch.setDeviceId(pending)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [candidateDevices])

  // ── Seed ──────────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!seed) return
    // `action` wins over `issues` when both arrive; the hidden Chat builtin
    // is the no-subject state, not a chip.
    if (seed.actionId && seed.actionId !== BUILTIN_CHAT_ID) {
      seededRepoActionId.current = null
      setSubject({
        kind: `action`,
        id: seed.actionId,
        inputs: seed.icon ? { icon: seed.icon } : {},
      })
      setSeedPrIssueId(seed.prIssueId)
    } else if (seed.issueIds.length > 0) {
      setSubject({ kind: `issues`, ids: [...new Set(seed.issueIds)] })
      setSeedPrIssueId(undefined)
    }
    if (seed.deviceId) {
      if (
        candidateDevices.some((candidate) => candidate.deviceId === seed.deviceId)
      ) {
        launch.setDeviceId(seed.deviceId)
      } else {
        pendingDeviceRef.current = seed.deviceId
      }
    }
    // The text lands only in an EMPTY draft — a seed never stomps typing.
    if (seed.text) {
      const seeded = seed.text
      setText((current) => (current.trim() ? current : seeded))
    }
    onSeedConsumed()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed])

  // Seed repo-typed inputs from the selected action's bound repository, once
  // per selection (EXP-349). Effect-level rather than in `pickAction`
  // because a seeded action's Electric row may sync in later; the merge is
  // current-wins so the PrInputField seed (fix-conflicts path) and any
  // already-picked values are never stomped.
  useEffect(() => {
    if (subject?.kind !== `action`) return
    if (seededRepoActionId.current === subject.id) return
    // Row not synced yet — leave the latch unset so the arrival retries.
    if (!selectedAction) return
    seededRepoActionId.current = subject.id
    const seeded = repoInputSeed(selectedAction)
    if (Object.keys(seeded).length > 0) {
      setSubject((current) =>
        current?.kind === `action` && current.id === selectedAction.id
          ? { ...current, inputs: { ...seeded, ...current.inputs } }
          : current
      )
    }
  }, [subject, selectedAction])

  // ── Subject actions ───────────────────────────────────────────────────────

  const toggleIssue = useCallback((issueId: string) => {
    setSubject((current) => {
      // Picking an issue while an action is the subject SWAPS (EXP-825:
      // exclusivity by swap, the chips make it visible).
      if (current?.kind !== `issues`) return { kind: `issues`, ids: [issueId] }
      const ids = current.ids.includes(issueId)
        ? current.ids.filter((id) => id !== issueId)
        : [...current.ids, issueId]
      return ids.length === 0 ? null : { kind: `issues`, ids }
    })
    setSeedPrIssueId(undefined)
  }, [])

  const pickAction = useCallback((actionId: string) => {
    if (actionId === BUILTIN_CHAT_ID) {
      setSubject(null)
      return
    }
    setSubject((current) =>
      current?.kind === `action` && current.id === actionId
        ? current
        : // A different action has a different input schema — stale values
          // must not leak into the new one's payload.
          { kind: `action`, id: actionId, inputs: {} }
    )
    setSeedPrIssueId(undefined)
  }, [])

  const clearAction = useCallback(() => {
    setSubject((current) => (current?.kind === `action` ? null : current))
    setSeedPrIssueId(undefined)
  }, [])

  const setInput = useCallback((key: string, value: string) => {
    setSubject((current) =>
      current?.kind === `action`
        ? { ...current, inputs: { ...current.inputs, [key]: value } }
        : current
    )
  }, [])

  // ── Images ────────────────────────────────────────────────────────────────

  const addFiles = useCallback(
    (files: File[], caret: number) => {
      const staged = stagePendingImages(imagesRef.current, files, text, caret)
      if (staged.added > 0) {
        setImages(staged.images)
        setText(staged.text)
      }
      if (staged.rejected > 0) {
        toast.error(`Only images up to 10 MB can be attached`)
      }
      if (staged.overflow > 0) {
        toast.error(`Up to ${MAX_STEER_IMAGES} images per message`)
      }
      return staged.caret
    },
    [text]
  )

  const removeImage = useCallback(
    (url: string) => {
      const dropped = dropPendingImage(imagesRef.current, url, text)
      if (dropped.images !== imagesRef.current) URL.revokeObjectURL(url)
      setImages(dropped.images)
      setText(dropped.text)
    },
    [text]
  )

  // Blob URLs are ours to revoke when the page goes.
  useEffect(
    () => () => {
      for (const image of imagesRef.current) URL.revokeObjectURL(image.url)
    },
    []
  )

  // ── Resume (EXP-481) ──────────────────────────────────────────────────────

  const { data: worktreeRows } = useLiveQuery((query) =>
    query.from({ w: deviceWorktreeCollection })
  )
  const soleIssue =
    subject?.kind === `issues` && subject.ids.length === 1
      ? allById.get(subject.ids[0]!)
      : undefined
  const soleIssueId = soleIssue?.id
  useEffect(() => {
    setResume(true)
  }, [soleIssueId])
  const resumeCandidate =
    soleIssue && device
      ? resumeWorktree(
          (worktreeRows ?? []) as SyncedDeviceWorktree[],
          device.rowId,
          soleIssue.identifier,
          agent
        )
      : null
  const resumeActive = resume && resumeCandidate !== null

  // ── Gate ──────────────────────────────────────────────────────────────────

  const count = checkedIds.length
  const overCap = count > MAX_ISSUES_PER_RUN
  const inputDefs = selectedAction?.inputs ?? []
  const missingInputs =
    subject?.kind === `action`
      ? missingRequiredInputs(inputDefs, subject.inputs)
      : []
  const hasText = text.trim().length > 0
  const busy = sending || remote.starting
  const subjectBlocked =
    subject === null
      ? // A chat needs SOMETHING to say — text or an image (EXP-739: the
        // repo is optional).
        !hasText && images.length === 0
      : subject.kind === `issues`
        ? overCap || spansRepos || spansBranches
        : !selectedAction ||
          missingInputs.length > 0 ||
          // The creator derives everything from the request text.
          (subject.id === BUILTIN_CREATE_ACTION_ID && !hasText)
  const blocked =
    busy ||
    !device ||
    // EXP-773: the picked agent is not ACP-ready on the picked machine, so
    // there is no transport left to start it on (the options line says so).
    launch.agentNotReady ||
    subjectBlocked

  // ── Submit ────────────────────────────────────────────────────────────────

  const submit = async () => {
    if (blocked || !device) return
    setSending(true)
    try {
      // Upload sequentially, persisting each id as it lands — a mid-batch
      // failure keeps the composer intact and a retry only uploads the rest.
      const ids: string[] = []
      let current = imagesRef.current
      try {
        for (const image of current) {
          let uploadedId = image.uploadedId
          if (!uploadedId) {
            const uploaded = await uploadTeamSessionImageFile(teamId, image.file)
            uploadedId = uploaded.id
            current = markPendingImageUploaded(current, image.url, uploadedId)
            setImages(current)
          }
          ids.push(uploadedId)
        }
      } catch (error) {
        toast.error(`Couldn't upload image`, {
          description: error instanceof Error ? error.message : undefined,
        })
        return
      }
      const prompt = buildSteerImageMessage(text, ids)
      const options = launch.buildOptions({ resume: resumeActive })
      // The remote hook toasts its own failures and rethrows; a refused start
      // keeps the draft so it can be retried.
      if (subject === null) {
        await remote.runAction(
          device,
          // The hidden builtin has no DB row, so the identity is constructed
          // here (the server re-derives it from the reserved id).
          { id: BUILTIN_CHAT_ID, name: BUILTIN_CHAT_NAME, teamId },
          options,
          chatStartInputs(repoId),
          prompt
        )
      } else if (subject.kind === `issues`) {
        await remote.startIssues(device, options, subject.ids, prompt || undefined)
      } else if (selectedAction) {
        await remote.runAction(
          device,
          {
            id: selectedAction.id,
            name: selectedAction.name,
            teamId: selectedAction.teamId,
          },
          options,
          buildInputsPayload(inputDefs, subject.inputs),
          prompt || undefined
        )
      }
      for (const image of current) URL.revokeObjectURL(image.url)
      setImages([])
      setText(``)
      setSubject(null)
      setSeedPrIssueId(undefined)
    } catch {
      // Already toasted by the remote hook.
    } finally {
      setSending(false)
    }
  }

  return {
    teamId,
    subject,
    selectedAction,
    actions,
    eligibleIssues,
    checkedIssues,
    toggleIssue,
    pickAction,
    clearAction,
    setInput,
    seedPrIssueId,
    text,
    setText,
    images,
    addFiles,
    removeImage,
    repos,
    repoId,
    setRepoId,
    repoOptions,
    resumeCandidate: resumeCandidate
      ? { identifier: soleIssue!.identifier, branch: resumeCandidate.branch }
      : null,
    resume,
    setResume,
    resumeActive,
    launch,
    candidateDevices,
    mcpServers: mcp.servers,
    mcpNow,
    submitLabel: submitLabelFor(subject),
    blocked,
    overCap,
    spansRepos,
    spansBranches,
    costHint: !overCap && !spansRepos && count > BATCH_COST_HINT_THRESHOLD,
    busy,
    sentTo: remote.sentTo,
    submit,
  }
}
