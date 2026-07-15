import { useMemo, useState } from "react"
import {
  Check,
  Flag,
  ListTodo,
  Minus,
  Tag,
  Trash2,
  UserRound,
  X,
} from "lucide-react"
import type { Issue, Label, User } from "@/db/schema"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Separator } from "@/components/ui/separator"

// Linear-style floating bulk action bar: shown while the issue list has a
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

export function BulkActionBar({
  issues,
  issueLabelMap,
  labels,
  users,
  onClear,
}: BulkActionBarProps) {
  const [busy, setBusy] = useState(false)
  const issueIds = useMemo(() => issues.map((issue) => issue.id), [issues])

  const orderedUsers = useMemo(
    () => [...users].sort((left, right) => left.name.localeCompare(right.name)),
    [users]
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

  const applyStatus = (status: IssueStatus) =>
    runBulk(
      (ids) => trpc.issues.bulkUpdate.mutate({ ids, status }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )

  const applyPriority = (priority: IssuePriority) =>
    runBulk(
      (ids) => trpc.issues.bulkUpdate.mutate({ ids, priority }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )

  const applyAssignee = (assigneeId: string | null) =>
    runBulk(
      (ids) => trpc.issues.bulkUpdate.mutate({ ids, assigneeId }),
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

  const deleteSelected = async () => {
    const ran = await runBulk(
      (ids) => trpc.issues.bulkDelete.mutate({ ids }),
      (txId) => issueCollection.utils.awaitTxId(txId)
    )
    if (ran) onClear()
  }

  return (
    <div
      className="fixed bottom-5 left-1/2 z-50 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-border/60 bg-popover/95 px-2 py-1.5 shadow-2xl"
      data-testid="bulk-action-bar"
    >
      <span className="px-1.5 text-xs font-medium whitespace-nowrap">
        {issues.length} selected
      </span>
      <Button
        variant="ghost"
        size="icon-xs"
        className="text-muted-foreground"
        aria-label="Clear selection"
        onClick={onClear}
      >
        <X className="size-3.5" />
      </Button>

      <Separator orientation="vertical" className="mx-1 h-4!" />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground"
            disabled={busy}
          >
            <ListTodo className="size-4" />
            Status
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-[11rem]">
          {/* No `duplicate` here: bulk marking has no canonical-issue picker,
              and status='duplicate' without duplicateOfId breaks the pairing
              invariant (single-issue paths intercept via the picker). */}
          {issueStatusOptions
            .filter((option) => option.value !== `duplicate`)
            .map((option) => {
              const Icon = option.icon
              return (
                <DropdownMenuItem
                  key={option.value}
                  onSelect={() => void applyStatus(option.value)}
                >
                  <Icon className={`size-4 ${option.color}`} />
                  {option.label}
                </DropdownMenuItem>
              )
            })}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground"
            disabled={busy}
          >
            <Flag className="size-4" />
            Priority
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-[11rem]">
          {issuePriorityOptions.map((option) => {
            const Icon = option.icon
            return (
              <DropdownMenuItem
                key={option.value}
                onSelect={() => void applyPriority(option.value)}
              >
                <Icon className={`size-4 ${option.color}`} />
                {option.label}
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground"
            disabled={busy}
          >
            <UserRound className="size-4" />
            Assignee
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-[13rem]">
          <DropdownMenuItem onSelect={() => void applyAssignee(null)}>
            <X className="size-4 text-muted-foreground" />
            Unassigned
          </DropdownMenuItem>
          {orderedUsers.map((user) => (
            <DropdownMenuItem
              key={user.id}
              onSelect={() => void applyAssignee(user.id)}
            >
              <Avatar className="size-5">
                {user.image && <AvatarImage src={user.image} alt={user.name} />}
                <AvatarFallback className="text-[0.5625rem]">
                  {getInitials(user.name)}
                </AvatarFallback>
              </Avatar>
              <span className="truncate">{user.name}</span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground"
            disabled={busy}
          >
            <Tag className="size-4" />
            Labels
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="w-[13rem]">
          {labels.length === 0 ? (
            <DropdownMenuItem disabled>No labels yet</DropdownMenuItem>
          ) : (
            labels.map((label) => {
              const state = labelState(label)
              return (
                <DropdownMenuItem
                  key={label.id}
                  // preventDefault keeps the menu open across toggles so a
                  // multi-label sweep is one visit.
                  onSelect={(event) => {
                    event.preventDefault()
                    void toggleLabel(label)
                  }}
                >
                  <span className="flex size-4 shrink-0 items-center justify-center">
                    {state === `all` ? (
                      <Check className="size-4" />
                    ) : state === `some` ? (
                      <Minus className="size-4 text-muted-foreground" />
                    ) : null}
                  </span>
                  <div
                    className="size-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: label.color }}
                  />
                  <span className="truncate">{label.name}</span>
                </DropdownMenuItem>
              )
            })
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <Separator orientation="vertical" className="mx-1 h-4!" />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            disabled={busy}
          >
            <Trash2 className="size-4" />
            Delete
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="end" className="w-[14rem]">
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
  )
}
