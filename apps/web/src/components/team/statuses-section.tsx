import { useEffect, useMemo, useState } from "react"
import { useLiveQuery, eq, inArray } from "@tanstack/react-db"
import {
  Ban,
  ChevronDown,
  ChevronUp,
  Lock,
  Ellipsis,
  Plus,
  Trash2,
} from "lucide-react"
import {
  issueCollection,
  issueStatusCollection,
  teamCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import type { Issue } from "@/db/schema"
import {
  ISSUE_STATUS_STARTED_MAX,
  issueStatusCategoryDisplayOrder,
  type IssueStatusCategory,
} from "@/lib/domain"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import { resolveIssueStatus, type StatusRowOption } from "@/lib/team-statuses"
import {
  StatusIcon,
  toStatusMenuOptions,
} from "@/components/issue-properties/status-dropdown"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { IconTooltip } from "@/components/icon-tooltip"
import { hexWithAlpha } from "@/lib/status-icons"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import {
  GlassGroup,
  GlassRow,
  GlassSectionHeader,
} from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { LABEL_COLORS, STATUS_COLORS } from "@/lib/label-colors"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"

const CATEGORY_LABEL: Record<IssueStatusCategory, string> = {
  backlog: `Backlog`,
  unstarted: `Unstarted`,
  started: `Started`,
  completed: `Completed`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

/** The 10%-alpha tile behind a status glyph — the settings-page echo of the
 * board's group-header wash. Builtins keep their token color class. */
function StatusTile({ option }: { option: StatusRowOption }) {
  return (
    <span
      className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md"
      style={
        option.builtinKey
          ? undefined
          : { backgroundColor: hexWithAlpha(option.colorHex, 0.1) }
      }
    >
      <StatusIcon option={option} />
    </span>
  )
}

/**
 * ONE live query over the team's synced issues, bucketed by resolved group
 * key. Scoped through the team's BOARDS — the issues shape deliberately
 * excludes team_id (REV2-5 scoping column), so a teamId filter would match
 * nothing client-side. These are board-VISIBLE counts: the server counts ALL
 * referencing rows (trashed boards included), so a row reading 0 here can
 * still hold issues — the delete dialog shows statuses.referencingCount.
 */
function useIssueCountsByStatus(
  teamId: string,
  options: StatusRowOption[]
): Map<string, number> {
  const boards = useTeamBoards(teamId)
  const boardIds = useMemo(() => boards.map((board) => board.id), [boards])
  const { data: rows } = useLiveQuery(
    (q) =>
      boardIds.length > 0
        ? q
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`)]
  )
  return useMemo(() => {
    const counts = new Map<string, number>(
      options.map((option) => [option.id, 0])
    )
    for (const issue of (rows ?? []) as Issue[]) {
      const key = resolveIssueStatus(issue, options).id
      counts.set(key, (counts.get(key) ?? 0) + 1)
    }
    return counts
  }, [rows, options])
}

function StatusRow({
  teamId,
  option,
  count,
  isFirst,
  isLast,
  isDefault,
  onRequestDelete,
}: {
  teamId: string
  option: StatusRowOption
  count: number
  isFirst: boolean
  isLast: boolean
  isDefault: boolean
  onRequestDelete: (option: StatusRowOption, count: number) => void
}) {
  const [name, setName] = useState(option.name)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const isBuiltin = option.builtinKey !== null

  const persistName = async () => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === option.name) {
      setName(option.name)
      setError(null)
      return
    }
    setBusy(true)
    try {
      const { txId } = await trpc.statuses.update.mutate({
        teamId,
        statusId: option.id,
        name: trimmed,
      })
      await issueStatusCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      setName(option.name)
      setError(err instanceof Error ? err.message : `Failed to rename status.`)
    } finally {
      setBusy(false)
    }
  }

  const persistColor = async (color: string) => {
    if (color === option.colorHex) return
    setBusy(true)
    try {
      const { txId } = await trpc.statuses.update.mutate({
        teamId,
        statusId: option.id,
        color,
      })
      await issueStatusCollection.utils.awaitTxId(txId)
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to set color.`)
    } finally {
      setBusy(false)
    }
  }

  const move = async (direction: `up` | `down`) => {
    setBusy(true)
    try {
      const { txId } = await trpc.statuses.move.mutate({
        teamId,
        statusId: option.id,
        direction,
      })
      await issueStatusCollection.utils.awaitTxId(txId)
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to move status.`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <GlassRow className="flex-col items-stretch gap-0 px-3 py-2">
      <div className="flex items-center gap-3">
        {isBuiltin ? (
          <StatusTile option={option} />
        ) : (
          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label={`Change color of ${option.name}`}
                disabled={busy}
                className="shrink-0 rounded-md hover:opacity-80"
              >
                <StatusTile option={option} />
              </button>
            </PopoverTrigger>
            {/* Bounded so the swatch grid WRAPS — `w-auto` let it stretch the
                popover to one 20-swatch row wider than a phone screen. */}
            <PopoverContent className="w-64 p-2" align="start">
              <ColorSwatchGrid
                colors={STATUS_COLORS}
                value={option.colorHex}
                onChange={persistColor}
              />
            </PopoverContent>
          </Popover>
        )}

        {isBuiltin ? (
          <span className="flex-1 truncate px-1 text-sm">{option.name}</span>
        ) : (
          <Input
            value={name}
            aria-label={`Status name`}
            onChange={(e) => {
              setName(e.target.value)
              setError(null)
            }}
            onBlur={persistName}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                ;(e.target as HTMLInputElement).blur()
              }
              if (e.key === `Escape`) {
                setName(option.name)
                setError(null)
                ;(e.target as HTMLInputElement).blur()
              }
            }}
            disabled={busy}
            className="h-7 flex-1 border-none bg-transparent px-1 shadow-none focus-visible:ring-0"
          />
        )}

        {isDefault && (
          <Pill className="font-normal">Default</Pill>
        )}
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          {count} issue{count === 1 ? `` : `s`}
        </span>

        {/* The lock slot is reserved on custom rows too, so the count, the
            Default badge and the name column line up across the section. */}
        {isBuiltin ? (
          <IconTooltip label="Built-in status: reorderable, but not renamable, recolorable or deletable.">
            <span className="flex h-7 w-7 items-center justify-center text-muted-foreground">
              <Lock className="h-3.5 w-3.5" />
            </span>
          </IconTooltip>
        ) : (
          <span aria-hidden className="h-7 w-7 shrink-0" />
        )}

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="glass"
              size="icon-sm"
              disabled={busy}
              aria-label={`Status actions for ${option.name}`}
            >
              <Ellipsis />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              disabled={isFirst}
              onSelect={() => void move(`up`)}
            >
              <ChevronUp className="h-4 w-4" />
              Move up
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={isLast}
              onSelect={() => void move(`down`)}
            >
              <ChevronDown className="h-4 w-4" />
              Move down
            </DropdownMenuItem>
            {!isBuiltin && (
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => onRequestDelete(option, count)}
              >
                <Trash2 className="h-4 w-4" />
                Delete
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {error && <p className="mt-1 px-1 text-xs text-destructive">{error}</p>}
    </GlassRow>
  )
}

function ReassignDialog({
  teamId,
  target,
  options,
  onOpenChange,
  onDeleted,
}: {
  teamId: string
  target: { option: StatusRowOption; count: number } | null
  options: StatusRowOption[]
  onOpenChange: (open: boolean) => void
  onDeleted: () => void
}) {
  const candidates = options.filter(
    (option) =>
      option.category !== `duplicate` && option.id !== target?.option.id
  )
  const backlogDefault =
    candidates.find((option) => option.builtinKey === `backlog`) ??
    candidates[0]
  const [reassignToId, setReassignToId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Server-authoritative referencing count — the synced count undershoots
  // whenever issues sit on trashed boards (they never sync). null = loading
  // or fetch failed; the copy then stays hedged instead of showing a number.
  const [serverCount, setServerCount] = useState<number | null>(null)
  const targetStatusId = target?.option.id ?? null
  useEffect(() => {
    setServerCount(null)
    if (!targetStatusId) return
    let cancelled = false
    trpc.statuses.referencingCount
      .query({ teamId, statusId: targetStatusId })
      .then(({ count }) => {
        if (!cancelled) setServerCount(count)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [teamId, targetStatusId])
  // EXP-319 pins live on the synced teams row, and their FK is ON DELETE SET
  // NULL — deleting a pinned status silently reverts that automation to the
  // builtin default. Correct, but invisible, so say it out loud here. Same
  // resolution as PrAutomationCard: an automation switched off ("do nothing")
  // is unaffected, so it stays quiet.
  const { data: teamRows } = useLiveQuery(
    (query) =>
      query
        .from({ teams: teamCollection })
        .where(({ teams }) => eq(teams.id, teamId)),
    [teamId]
  )
  const team = teamRows?.[0]
  const prPinNotice = useMemo(() => {
    if (!team || !targetStatusId) return null
    const hits = PR_AUTOMATION_ROWS.filter(({ event }) => {
      const pinned =
        event === `pr_opened` ? team.prOpenedStatusId : team.prMergedStatusId
      const automation =
        event === `pr_opened`
          ? team.prOpenedAutomation
          : team.prMergedAutomation
      return automation !== false && pinned === targetStatusId
    })
    if (hits.length === 0) return null
    const fallbacks = hits.map(
      ({ defaultKey }) =>
        options.find((option) => option.builtinKey === defaultKey)?.name ?? null
    )
    const named = fallbacks.every((name) => name !== null)
    const targets = named
      ? fallbacks.join(` and `)
      : hits.length > 1
        ? `the default statuses`
        : `the default status`
    return hits.length > 1
      ? `This status is where issues move when a pull request opens or merges. Those automations will fall back to ${targets}.`
      : `This status is where issues move when a pull request ${hits[0].verb}. That automation will fall back to ${targets}.`
  }, [team, targetStatusId, options])
  // Self-correcting: a stale pick from a previous target (or the row being
  // deleted) falls back to the team's Backlog builtin.
  const selectedId =
    (reassignToId && candidates.some((o) => o.id === reassignToId)
      ? reassignToId
      : null) ??
    backlogDefault?.id ??
    null

  const confirm = async () => {
    if (!target || !selectedId) return
    setBusy(true)
    try {
      const { txId } = await trpc.statuses.delete.mutate({
        teamId,
        statusId: target.option.id,
        reassignToId: selectedId,
      })
      await issueStatusCollection.utils.awaitTxId(txId)
      onDeleted()
      onOpenChange(false)
      setReassignToId(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to delete status.`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent mobile="alert" className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Delete {target?.option.name}?</DialogTitle>
          <DialogDescription>
            {serverCount === null
              ? `Issues using this status (including any on trashed boards) will move to the status you pick.`
              : serverCount === 0
                ? `No issues use this status right now. Anything referencing it when you confirm will move to the status you pick.`
                : `${serverCount} issue${serverCount === 1 ? `` : `s`}${serverCount > (target?.count ?? 0) ? ` (some on trashed boards)` : ``} will move to the status you pick.`}
          </DialogDescription>
        </DialogHeader>
        {/* The reassign list is the only growing part — it scrolls inside the
            DialogBody so the confirm buttons stay pinned (EXP-369). */}
        <DialogBody className="space-y-3">
          {prPinNotice && (
            <p className="rounded-md border bg-muted/40 px-2.5 py-2 text-xs text-muted-foreground">
              {prPinNotice}
            </p>
          )}
          <div className="space-y-1">
            {candidates.map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => setReassignToId(option.id)}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm hover:bg-accent ${
                  selectedId === option.id ? `bg-accent` : ``
                }`}
              >
                <StatusIcon option={option} />
                <span className="truncate">{option.name}</span>
              </button>
            ))}
          </div>
          {error && <p className="text-xs text-destructive">{error}</p>}
        </DialogBody>
        <DialogFooter>
          <DialogCancel disabled={busy} onClick={() => onOpenChange(false)} />
          <Button
            type="button"
            variant="destructive"
            disabled={busy || !selectedId}
            onClick={() => void confirm()}
          >
            {busy ? `Deleting…` : `Delete status`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function CreateStatusForm({
  teamId,
  category,
  onDone,
}: {
  teamId: string
  category: IssueStatusCategory
  onDone: () => void
}) {
  const [name, setName] = useState(``)
  const [color, setColor] = useState(LABEL_COLORS[6])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const create = async () => {
    const trimmed = name.trim()
    if (!trimmed || busy) return
    setBusy(true)
    try {
      const { txId } = await trpc.statuses.create.mutate({
        teamId,
        category,
        name: trimmed,
        color,
      })
      await issueStatusCollection.utils.awaitTxId(txId)
      onDone()
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to create status.`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <GlassRow className="mt-2 flex-col items-stretch gap-3">
      <Input
        value={name}
        onChange={(e) => {
          setName(e.target.value)
          setError(null)
        }}
        placeholder={`New ${CATEGORY_LABEL[category].toLowerCase()} status`}
        autoFocus
        className="h-8 text-sm"
        onKeyDown={(e) => {
          if (e.key === `Enter`) {
            e.preventDefault()
            void create()
          }
          if (e.key === `Escape`) onDone()
        }}
      />
      {error && <p className="text-xs text-destructive">{error}</p>}
      <div>
        <span className="mb-1.5 block text-xs text-muted-foreground">
          Color
        </span>
        <ColorSwatchGrid
          colors={STATUS_COLORS}
          value={color}
          onChange={setColor}
        />
      </div>
      <div className="flex items-center gap-2">
        <Button
          size="xs"
          variant="default"
          disabled={!name.trim() || busy}
          onClick={() => void create()}
        >
          {busy ? `Creating...` : `Create status`}
        </Button>
        <Pill mode="action" disabled={busy} onClick={onDone}>
          Cancel
        </Pill>
      </div>
    </GlassRow>
  )
}

// EXP-319 — per-team PR automation targets. Two pickers over the team's
// statuses (duplicate excluded, it needs a canonical issue) plus a "Do
// nothing" entry. The synced teams row is the source of truth: a NULL
// status_id means the builtin default (In Review / Done — also what the
// FK's SET NULL falls back to when a target status is deleted), and
// *_automation=false means "do nothing".
const PR_AUTOMATION_ROWS = [
  {
    event: `pr_opened`,
    label: `When a pull request opens`,
    verb: `opens`,
    defaultKey: `in_review`,
  },
  {
    event: `pr_merged`,
    label: `When a pull request merges`,
    verb: `merges`,
    defaultKey: `done`,
  },
] as const

function PrAutomationCard({
  teamId,
  options,
}: {
  teamId: string
  options: StatusRowOption[]
}) {
  const { data: teamRows } = useLiveQuery(
    (query) =>
      query
        .from({ teams: teamCollection })
        .where(({ teams }) => eq(teams.id, teamId)),
    [teamId]
  )
  const team = teamRows?.[0]
  const [error, setError] = useState<string | null>(null)

  const pickable = useMemo(
    () => options.filter((option) => option.category !== `duplicate`),
    [options]
  )
  const menuOptions = useMemo(
    () => [
      ...toStatusMenuOptions(pickable),
      {
        value: `none`,
        label: `Do nothing`,
        icon: Ban,
        color: `text-muted-foreground`,
      },
    ],
    [pickable]
  )

  const persist = async (
    event: `pr_opened` | `pr_merged`,
    target: string,
    current: string
  ) => {
    if (target === current) return
    try {
      const { txId } = await trpc.statuses.setPrAutomation.mutate({
        teamId,
        event,
        target,
      })
      await teamCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      setError(
        err instanceof Error ? err.message : `Failed to update PR automation.`
      )
    }
  }

  if (!team || pickable.length === 0) return null

  return (
    <div>
      <GlassSectionHeader label="PR automation" />
      <GlassGroup>
          {PR_AUTOMATION_ROWS.map(({ event, label, defaultKey }) => {
            const statusId =
              event === `pr_opened`
                ? team.prOpenedStatusId
                : team.prMergedStatusId
            const automation =
              event === `pr_opened`
                ? team.prOpenedAutomation
                : team.prMergedAutomation
            const defaultId =
              pickable.find((option) => option.builtinKey === defaultKey)?.id ??
              `none`
            const value =
              automation === false
                ? `none`
                : statusId && pickable.some((option) => option.id === statusId)
                  ? statusId
                  : defaultId

            return (
              <div
                key={event}
                className="flex items-center justify-between gap-3 px-4 py-3"
              >
                <span className="min-w-0 text-sm">{label}, move issues to</span>
                <OptionDropdownMenu
                  value={value}
                  fallbackValue={defaultId}
                  options={menuOptions}
                  mobileTitle={label}
                  align="end"
                  onSelect={(picked) => void persist(event, picked, value)}
                  renderTrigger={(selected) => {
                    const Icon = selected.icon
                    return (
                      // Fixed width so both rows' triggers line up (EXP-328) —
                      // the label ellipsizes instead of stretching the button.
                      <Button
                        variant="outline"
                        size="sm"
                        className="w-44 shrink-0 justify-start"
                      >
                        <Icon
                          className={`h-4 w-4 ${selected.color}`}
                          style={
                            selected.colorHex
                              ? { color: selected.colorHex }
                              : undefined
                          }
                        />
                        <span className="flex-1 truncate text-left">
                          {selected.label}
                        </span>
                        <ChevronDown className="h-3 w-3 text-muted-foreground" />
                      </Button>
                    )
                  }}
                />
              </div>
            )
          })}
      </GlassGroup>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  )
}

export function TeamStatusesSection({ teamId }: { teamId: string }) {
  const { options, ready } = useTeamStatuses(teamId)
  const counts = useIssueCountsByStatus(teamId, options)
  const [creatingIn, setCreatingIn] = useState<IssueStatusCategory | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<{
    option: StatusRowOption
    count: number
  } | null>(null)

  // Delete ALWAYS goes through the one confirm-and-reassign dialog (EXP-320)
  // — even a 0-count status can be referenced by issues on trashed boards,
  // which never sync, so there is no safe "just delete it" fast path. The
  // dialog fetches the server-authoritative count and always sends a
  // reassignment target, so the delete can never bounce PRECONDITION_FAILED.
  const requestDelete = (option: StatusRowOption, count: number) => {
    setDeleteTarget({ option, count })
  }

  const byCategory = useMemo(() => {
    const map = new Map<IssueStatusCategory, StatusRowOption[]>()
    for (const category of issueStatusCategoryDisplayOrder)
      map.set(category, [])
    for (const option of options) map.get(option.category)?.push(option)
    return map
  }, [options])

  // The "Default" badge marks where a brand-new issue lands.
  const defaultOptionId = resolveIssueStatus(
    { status: `backlog`, statusId: null },
    options
  ).id

  // Until the shape syncs, `options` is the CONSTRUCTED fallback set, whose
  // synthetic `builtin:<key>` ids are not row uuids — every control here is
  // row-level, so a move/rename/delete on one would send `builtin:backlog` to the
  // server and bounce off its uuid check. Wait for real rows instead of
  // offering dead controls (desktop guards the same way, settings/statuses.rs).
  if (!ready) {
    return (
      <div>
        <GlassSectionHeader label="Statuses" />
        <p className="py-2 text-sm text-muted-foreground">Loading…</p>
      </div>
    )
  }

  return (
    <>
      <div className="space-y-6">
        <div className="space-y-6">
          {issueStatusCategoryDisplayOrder.map((category) => {
            const rows = byCategory.get(category) ?? []
            const atStartedCap =
              category === `started` &&
              rows.length >= ISSUE_STATUS_STARTED_MAX
            const canAdd = category !== `duplicate`

            return (
              <div key={category}>
                <GlassSectionHeader
                  label={CATEGORY_LABEL[category]}
                  trailing={
                    canAdd &&
                    (atStartedCap ? (
                      <IconTooltip
                        label={`A team can have at most ${ISSUE_STATUS_STARTED_MAX} started statuses.`}
                      >
                        <Button
                          variant="glass"
                          size="icon-sm"
                          disabled
                          aria-label={`Add ${CATEGORY_LABEL[category]} status`}
                        >
                          <Plus />
                        </Button>
                      </IconTooltip>
                    ) : (
                      <Button
                        variant="glass"
                        size="icon-sm"
                        onClick={() =>
                          setCreatingIn(
                            creatingIn === category ? null : category
                          )
                        }
                        aria-label={`Add ${CATEGORY_LABEL[category]} status`}
                      >
                        <Plus />
                      </Button>
                    ))
                  }
                />

                {rows.length === 0 && (
                  <p className="py-1 text-xs text-muted-foreground">
                    No statuses yet.
                  </p>
                )}
                <div className="space-y-2">
                  {rows.map((option, index) => (
                    <StatusRow
                      // Re-mount on rename/recolor so the inline editor's
                      // local state can never shadow a synced change
                      // (LabelRow convention).
                      key={`${option.id}:${option.name}:${option.colorHex}`}
                      teamId={teamId}
                      option={option}
                      count={counts.get(option.id) ?? 0}
                      isFirst={index === 0}
                      isLast={index === rows.length - 1}
                      isDefault={option.id === defaultOptionId}
                      onRequestDelete={requestDelete}
                    />
                  ))}
                </div>

                {creatingIn === category && (
                  <CreateStatusForm
                    teamId={teamId}
                    category={category}
                    onDone={() => setCreatingIn(null)}
                  />
                )}
              </div>
            )
          })}
        </div>

        <PrAutomationCard teamId={teamId} options={options} />
      </div>

      <ReassignDialog
        teamId={teamId}
        target={deleteTarget}
        options={options}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
        onDeleted={() => setDeleteTarget(null)}
      />
    </>
  )
}
