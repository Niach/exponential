import { useEffect, useMemo, useState } from "react"
import { Flag, Trash2, X } from "lucide-react"
import type { Issue, Label, User } from "@/db/schema"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import {
  conceptIcon,
  Button,
  ComboboxMenuItems,
  Pill,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Separator,
  UserAvatar,
  type PickerOption,
} from "@exp/ui"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"
import { useMobileChrome } from "@/hooks/use-mobile-chrome"
import { useSession } from "@/hooks/use-session"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSteerConfig } from "@/components/agent-session"
import { useIsTeamMember } from "@/components/issue-coding-rows"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { trpc } from "@/lib/trpc-client"
import { issuePriorityOptions } from "@/lib/domain"
import type { IssuePriority } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  statusUpdatePayload,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import { displayUserName } from "@/lib/user-display"

// Bulk action bar: rendered by the board / My Issues views as an in-flow row
// at the top of the list (in the header region) while the issue list has a
// multi-selection. Property edits (status/priority/assignee/labels)
// keep the selection alive — only delete clears it (Linear semantics; the
// desktop bar mirrors this). Every mutation goes through the bulk tRPC
// procedures, chunked at the server's 200-id cap, awaiting the LAST txId so
// Electric has echoed every row version before the UI settles.
interface BulkActionBarProps {
  // Selected issues, in visible list order.
  issues: Issue[]
  issueLabelMap: Map<string, Label[]>
  labels: Label[]
  users: User[]
  // Scopes the "Start coding" gates (membership, boards, devices) — My Issues
  // spans boards, so the selection alone cannot name the team.
  teamId: string
  onClear: () => void
}

const BULK_CHUNK_SIZE = 200

function chunk<T>(items: T[], size: number): T[][] {
  const chunks: T[][] = []
  for (let i = 0; i < items.length; i += size) {
    chunks.push(items.slice(i, i + size))
  }
  return chunks
}

// EXP-957: what the WHOLE selection already reads as for one property — the
// value every selected issue shares, or `mixed` when they disagree. `null` is
// a real value for a nullable field (an unassigned issue), so the two are
// reported separately: `mixed` is what the menus pass as `indeterminate`, and
// it is the only thing that tells "all unassigned" from "they disagree".
function sharedValue<T>(
  issues: Issue[],
  read: (issue: Issue) => T
): { value: T | null; mixed: boolean } {
  const first = issues.length > 0 ? read(issues[0]!) : null
  const mixed = issues.some((issue) => read(issue) !== first)
  return { value: mixed ? null : first, mixed }
}

export function BulkActionBar({
  issues,
  issueLabelMap,
  labels,
  users,
  teamId,
  onClear,
}: BulkActionBarProps) {
  const [busy, setBusy] = useState(false)
  const issueIds = useMemo(() => issues.map((issue) => issue.id), [issues])
  const {
    options: teamStatusOptions,
    byId: statusById,
    resolve: resolveStatus,
  } = useTeamStatusesContext()

  // EXP-698 r5: while a selection lives, THIS bar is the phone's bottom
  // chrome — the tab bar and its FAB step aside (`use-mobile-chrome.tsx`),
  // and the list spends `--bulkbar-h` instead of `--tabbar-h`.
  const { setBulkBarPresent } = useMobileChrome()
  useEffect(() => {
    setBulkBarPresent(true)
    return () => setBulkBarPresent(false)
  }, [setBulkBarPresent])
  const publishBulkBarHeight = useChromeHeightVar(`--bulkbar-h`)

  const orderedUsers = useMemo(
    () => [...users].sort((left, right) => left.name.localeCompare(right.name)),
    [users]
  )

  // EXP-957 — the menu rows and the value they mark. `statusRows` stays around
  // beside its menu options because a pick reports an id and `applyStatus`
  // needs the ROW (the fallback set's synthetic ids are not in `statusById`).
  const statusRows = useMemo(
    () => creatableStatusOptions(teamStatusOptions),
    [teamStatusOptions]
  )
  const statusMenuOptions = useMemo(
    () => toStatusMenuOptions(statusRows),
    [statusRows]
  )
  const sharedStatus = useMemo(
    () => sharedValue(issues, (issue) => resolveStatus(issue).id),
    [issues, resolveStatus]
  )
  const sharedPriority = useMemo(
    () => sharedValue(issues, (issue) => issue.priority),
    [issues]
  )
  const sharedAssignee = useMemo(
    () => sharedValue(issues, (issue) => issue.assigneeId),
    [issues]
  )

  const usersById = useMemo(
    () => new Map(orderedUsers.map((user) => [user.id, user])),
    [orderedUsers]
  )
  const assigneeOptions = useMemo<PickerOption[]>(
    () =>
      orderedUsers.map((user) => {
        const name = displayUserName(user, user.id)
        return {
          value: user.id,
          label: name,
          keywords: [name, user.email ?? ``],
        }
      }),
    [orderedUsers]
  )

  // Sequential chunk loop; awaiting only the LAST txId is enough — Electric
  // replays transactions in commit order, so the last one landing implies
  // every earlier chunk landed too. Returns false when a run is in flight.
  const runBulk = async (
    execute: (ids: string[]) => Promise<{ txId: number }>,
    awaitTx: (txId: number) => Promise<unknown>
  ): Promise<boolean> => {
    if (busy) return false
    setBusy(true)
    try {
      let lastTxId: number | undefined
      for (const ids of chunk(issueIds, BULK_CHUNK_SIZE)) {
        const { txId } = await execute(ids)
        lastTxId = txId
      }
      if (lastTxId !== undefined) {
        await awaitTx(lastTxId)
      }
      return true
    } finally {
      setBusy(false)
    }
  }

  const applyStatus = (option: StatusRowOption) =>
    runBulk(
      (ids) =>
        trpc.issues.bulkUpdate.mutate({
          issueIds: ids,
          ...statusUpdatePayload(option),
        }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )

  const applyPriority = (priority: IssuePriority) =>
    runBulk(
      (ids) => trpc.issues.bulkUpdate.mutate({ issueIds: ids, priority }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )

  const applyAssignee = (assigneeId: string | null) =>
    runBulk(
      (ids) => trpc.issues.bulkUpdate.mutate({ issueIds: ids, assigneeId }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )

  // Tri-state label toggle: all-have → remove from all; else add to all.
  const labelState = (label: Label): `all` | `some` | `none` => {
    let count = 0
    for (const issue of issues) {
      if (
        (issueLabelMap.get(issue.id) ?? []).some((row) => row.id === label.id)
      ) {
        count += 1
      }
    }
    return count === issues.length ? `all` : count > 0 ? `some` : `none`
  }

  const toggleLabel = (label: Label) => {
    const removeFromAll = labelState(label) === `all`
    return runBulk(
      (ids) =>
        removeFromAll
          ? trpc.issueLabels.bulkRemove.mutate({
              labelId: label.id,
              issueIds: ids,
            })
          : trpc.issueLabels.bulkAdd.mutate({
              labelId: label.id,
              issueIds: ids,
            }),
      (txId) => issueLabelCollection.utils.awaitTxId(txId)
    )
  }

  // The multi arm's `checked` carries the tri-state (a label on SOME of the
  // selection is `"indeterminate"`); `value` stays the honest membership array
  // the toggle arithmetic runs on, so `onChange` reports exactly one changed
  // id — the row that was picked.
  const labelOptions = useMemo<PickerOption[]>(
    () =>
      labels.map((label) => {
        const state = labelState(label)
        return {
          value: label.id,
          label: label.name,
          dot: label.color,
          checked:
            state === `all`
              ? true
              : state === `some`
                ? (`indeterminate` as const)
                : false,
        }
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- labelState reads exactly these.
    [labels, issues, issueLabelMap]
  )
  const selectedLabelIds = useMemo(
    () =>
      labelOptions
        .filter((option) => option.checked === true)
        .map((option) => option.value),
    [labelOptions]
  )

  const toggleLabelId = (next: string[]) => {
    const before = new Set(selectedLabelIds)
    const after = new Set(next)
    const toggledId =
      next.find((id) => !before.has(id)) ??
      selectedLabelIds.find((id) => !after.has(id))
    const label = labels.find((row) => row.id === toggledId)
    if (label) void toggleLabel(label)
  }

  const deleteSelected = async () => {
    const ran = await runBulk(
      (ids) => trpc.issues.bulkDelete.mutate({ issueIds: ids }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )
    if (ran) onClear()
  }

  // Exactly one, not `<= 1` — same contract as the list's assignee column
  // (issue-list.tsx): `users` length 0 means the member list is still
  // syncing, and treating that as solo made the Assignee button vanish and
  // reappear on a genuine multi-member team.
  const isSolo = users.length === 1

  return (
    // POSITIONING ONLY. On md+ this wrapper is `display: contents`, so the bar
    // stays a direct child of the filter row exactly as before; below md it is
    // the fixed, centered box. The split exists because EXP-523's enter
    // animation writes `transform` (tw-animate-css's keyframes replace it
    // wholesale), which would cancel the `-translate-x-1/2` centering and snap
    // the bar half its width to the right on every phone selection.
    // EXP-698 r5: on phones the bar IS the tab bar now, so it copies the tab
    // bar's box exactly — pinned to `bottom-0` with the safe-area inset as its
    // own PADDING, never as an offset. That is what makes the measured
    // `--bulkbar-h` (published here, spent by TAB_BAR_CLEARANCE) cover the
    // same footprint `--tabbar-h` does; an inset expressed as `bottom-…` would
    // sit outside the measured box and under-reserve the list's clearance.
    <div
      ref={publishBulkBarHeight}
      className="md:contents max-md:fixed max-md:bottom-0 max-md:left-1/2 max-md:z-40 max-md:max-w-[calc(100vw-2rem)] max-md:-translate-x-1/2 max-md:pb-[max(1rem,env(safe-area-inset-bottom))]"
    >
      <div
        // EXP-698 r5: the opaque glass card of the tab bar and the natives'
        // selection bar — 24px radius over the strong hairline, 10×8 padding.
        // EXP-523: enter-only. Clearing a selection is a deliberate action and
        // reads fine instantly, and an exit would mean threading presence state
        // through both call sites (board view + my-issues) for no real gain.
        // Phone budget (416px viewport → 384px of bar): the separators go, the
        // gaps halve and Start coding shrinks, so × + count + four 32px icons
        // + the pill + the trash land around 340px — the natives' one-row
        // 360dp bar. `overflow-x-auto` is the safety net, not the plan: a
        // longer count or a translated label scrolls instead of pushing the
        // destructive button off the screen edge (EXP-698 r5 shot review).
        className="flex items-center gap-1 rounded-3xl border border-glass-stroke-strong bg-glass-card-opaque px-2.5 py-2 motion-safe:animate-in motion-safe:slide-in-from-bottom-1 motion-safe:fade-in-0 motion-safe:zoom-in-95 duration-fast ease-decelerate max-md:h-[52px] max-md:max-w-[calc(100vw-2rem)] max-md:gap-0.5 max-md:overflow-x-auto max-md:shadow-lg max-md:shadow-black/40"
        data-testid="bulk-action-bar"
      >
        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0 text-muted-foreground"
          aria-label="Clear selection"
          onClick={onClear}
        >
          <X className="size-4" />
        </Button>
        <span className="shrink-0 px-1 text-sm font-semibold whitespace-nowrap">
          {issues.length}
        </span>

        <Separator
          orientation="vertical"
          className="mx-1 h-4! max-md:hidden"
        />

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 text-muted-foreground max-md:w-8 max-md:px-0!"
              disabled={busy}
              aria-label="Set status"
            >
              <StatusIcon className="size-4" />
              <span className="hidden md:inline">Status</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            side="bottom"
            align="start"
            collisionPadding={12}
            className="w-[11rem]"
          >
            {/* EXP-957: the rows are the Combobox's MENU arm, so the mark
              shows what the whole selection ALREADY has — a bulk menu used to
              show no current value at all. Nothing is marked when the selected
              issues disagree (`indeterminate`).
              No duplicate-CATEGORY row here: bulk marking has no
              canonical-issue picker, and status='duplicate' without
              duplicateOfId breaks the pairing invariant (single-issue paths
              intercept via the picker). */}
            <ComboboxMenuItems
              menu="dropdown"
              options={statusMenuOptions}
              value={sharedStatus.value}
              indeterminate={sharedStatus.mixed}
              onChange={(statusId) => {
                if (statusId === null) return
                const row =
                  statusById.get(statusId) ??
                  statusRows.find((option) => option.id === statusId)
                if (row) void applyStatus(row)
              }}
            />
          </DropdownMenuContent>
        </DropdownMenu>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 text-muted-foreground max-md:w-8 max-md:px-0!"
              disabled={busy}
              aria-label="Set priority"
            >
              <Flag className="size-4" />
              <span className="hidden md:inline">Priority</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            side="bottom"
            align="start"
            collisionPadding={12}
            className="w-[11rem]"
          >
            <ComboboxMenuItems
              menu="dropdown"
              options={issuePriorityOptions}
              value={sharedPriority.value}
              indeterminate={sharedPriority.mixed}
              onChange={(priority) => {
                if (priority === null) return
                void applyPriority(priority)
              }}
            />
          </DropdownMenuContent>
        </DropdownMenu>

        {/* Hidden on solo teams (nothing to reassign); length 0 = still
          loading, also hidden. */}
        {!isSolo && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className="shrink-0 text-muted-foreground max-md:w-8 max-md:px-0!"
                disabled={busy}
                aria-label="Set assignee"
              >
                <AssigneeIcon className="size-4" />
                <span className="hidden md:inline">Assignee</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              side="bottom"
              align="start"
              collisionPadding={12}
              className="w-[13rem]"
            >
              {/* "Unassigned" is the primitive's none row now, and it is
                marked only when EVERY selected issue is unassigned. */}
              <ComboboxMenuItems
                menu="dropdown"
                options={assigneeOptions}
                value={sharedAssignee.value}
                indeterminate={sharedAssignee.mixed}
                noneLabel="Unassigned"
                onChange={(assigneeId) => void applyAssignee(assigneeId)}
                renderOption={(option) => (
                  <>
                    <UserAvatar
                      size={20}
                      user={{
                        id: option.value,
                        name: String(option.label),
                        image: usersById.get(option.value)?.image ?? null,
                      }}
                    />
                    <span className="min-w-0 flex-1 truncate text-sm">
                      {option.label}
                    </span>
                  </>
                )}
              />
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 text-muted-foreground max-md:w-8 max-md:px-0!"
              disabled={busy}
              aria-label="Set labels"
            >
              <LabelsIcon className="size-4" />
              <span className="hidden md:inline">Labels</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            side="bottom"
            align="start"
            collisionPadding={12}
            className="w-[13rem]"
          >
            {/* The tri-state (on all / on some / on none of the selection) is
              the primitive's circle-check / circle-minus / circle glyph, and
              the multi arm keeps the menu open by itself, so a multi-label
              sweep is still one visit. */}
            <ComboboxMenuItems
              menu="dropdown"
              multiple
              options={labelOptions}
              value={selectedLabelIds}
              onChange={toggleLabelId}
              emptyText="No labels yet"
            />
          </DropdownMenuContent>
        </DropdownMenu>

        <BulkStartCodingButton
          teamId={teamId}
          issues={issues}
          onClear={onClear}
        />

        <Separator
          orientation="vertical"
          className="mx-1 h-4! max-md:hidden"
        />

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 text-destructive hover:text-destructive max-md:w-8 max-md:px-0!"
              disabled={busy}
              aria-label="Delete selected"
            >
              <Trash2 className="size-4" />
              <span className="hidden md:inline">Delete</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            side="bottom"
            align="end"
            collisionPadding={12}
            className="w-[14rem]"
          >
            <DropdownMenuItem
              variant="destructive"
              onSelect={() => void deleteSelected()}
            >
              <Trash2 className="size-4" />
              {issues.length === 1
                ? `Confirm delete 1 issue`
                : `Confirm delete ${issues.length} issues`}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  )
}

const StartCodingIcon = conceptIcon(`action-run`)
// ×4 concepts (iOS `IssueListView` bulk bar): status, assignee, labels.
const StatusIcon = conceptIcon(`ui-checklist`)
const AssigneeIcon = conceptIcon(`ui-assignee`)
const LabelsIcon = conceptIcon(`settings-labels`)

// EXP-642: bulk "Start coding" — the desktop/iOS/Android selection bars have
// had it since EXP-439, only web lacked it. Gates, in order: member, relay
// configured, at least one selected issue on a REPO-BACKED board. That last
// one matters because the composer (EXP-825) seeds its chips from the ids
// but only LISTS repo-backed boards, so an unfiltered seed would silently
// include issues the composer can neither show nor start.
function BulkStartCodingButton({
  teamId,
  issues,
  onClear,
}: {
  teamId: string
  issues: Issue[]
  onClear: () => void
}) {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const steer = useSteerConfig()
  const isMember = useIsTeamMember(teamId, currentUserId ?? ``)
  const boards = useTeamBoards(teamId)

  const startableIds = useMemo(() => {
    const repoBacked = new Set(
      boards.filter((board) => board.repositoryId).map((board) => board.id)
    )
    return issues
      .filter((issue) => repoBacked.has(issue.boardId))
      .map((issue) => issue.id)
  }, [boards, issues])

  if (
    !currentUserId ||
    !isMember ||
    !steer?.enabled ||
    startableIds.length === 0
  ) {
    return null
  }
  return (
    <BulkStartCodingControl
      teamId={teamId}
      currentUserId={currentUserId}
      issueIds={startableIds}
      onClear={onClear}
    />
  )
}

// Split out so the device wiring (`useRemoteStart` over the synced devices
// shape) mounts only once the gates above passed — same posture as
// RemoteStartRow in issue-coding-rows.tsx.
function BulkStartCodingControl({
  teamId,
  currentUserId,
  issueIds,
  onClear,
}: {
  teamId: string
  currentUserId: string
  issueIds: string[]
  onClear: () => void
}) {
  const remote = useRemoteStart({ currentUserId, teamId })
  const openComposer = useOpenComposer()

  // Devices still resolving, or nothing to start on: stay quiet rather than
  // spend a slot in an already-crowded bar on an explanation (the issue view
  // carries that copy).
  if (remote.devices === null || remote.devices.length === 0) return null

  return (
    /* EXP-698 r5: the row's ONE call to action — the accent pill every
       client paints here, text and all, on phones too. EXP-825: it lands
       the selection on the Agent page composer as chips (2+ = a batch). */
    <Pill
      size="md"
      mode="action"
      primary
      className="mx-1 max-md:mx-0 max-md:gap-1 max-md:px-2.5 max-md:text-xs"
      aria-label="Start coding"
      onClick={() => {
        openComposer({ issueIds })
        // Desktop parity (EXP-439): a launched selection is done with.
        onClear()
      }}
    >
      <StartCodingIcon className="size-4" />
      Start coding
    </Pill>
  )
}
