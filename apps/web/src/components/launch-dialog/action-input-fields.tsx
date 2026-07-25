import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Check } from "lucide-react"
import {
  type ActionInputDef,
  MAX_ACTION_INPUT_TEXT,
} from "@exp/db-schema/domain"
import type { Board, Issue } from "@/db/schema"
import { boardCollection, issueCollection } from "@/lib/collections"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

// The selected action's typed input fields (EXP-257): text → plain Input,
// repo → compact Select over the team's connected repos, board → a
// MobilePopover + Command picker over the synced boards (the board-picker
// pattern), pr (EXP-259) → the same picker over the team's OPEN issue-linked
// pull requests (deduped by prUrl — a batch PR shows once, its value is the
// representative issue's id). Values live in the dialog shell as a flat
// Record<key, string> — repo/board/pr store the picked id, blank = unset
// (dropped from the payload by buildInputsPayload).

// Radix Select forbids an empty-string item value; the unset optional repo
// rides this sentinel inside the dialog only.
const NO_REPO = `none`

export function ActionInputFields({
  defs,
  values,
  onChange,
  repos,
  teamId,
}: {
  defs: ActionInputDef[]
  values: Record<string, string>
  onChange: (key: string, value: string) => void
  /** The team's connected repos, for `repo` inputs. */
  repos: ActionRepoOption[]
  teamId: string
}) {
  if (defs.length === 0) return null
  return (
    <div className="space-y-3">
      {defs.map((def) => {
        const label = def.required ? def.label : `${def.label} (optional)`
        const fieldId = `launch-action-input-${def.key}`
        if (def.type === `text`) {
          return (
            <div key={def.key} className="space-y-2">
              <Label htmlFor={fieldId}>{label}</Label>
              <Input
                id={fieldId}
                value={values[def.key] ?? ``}
                onChange={(e) => onChange(def.key, e.target.value)}
                placeholder={def.placeholder}
                // Client parity with the server's per-value cap, so a long
                // paste is refused at the field instead of at submit.
                maxLength={MAX_ACTION_INPUT_TEXT}
              />
            </div>
          )
        }
        if (def.type === `repo`) {
          return (
            <div key={def.key} className="space-y-2">
              <Label htmlFor={fieldId}>{label}</Label>
              <Select
                value={values[def.key] || (def.required ? `` : NO_REPO)}
                onValueChange={(value) =>
                  onChange(def.key, value === NO_REPO ? `` : value)
                }
              >
                <SelectTrigger id={fieldId} className="w-full">
                  <SelectValue placeholder="Select a repository" />
                </SelectTrigger>
                <SelectContent>
                  {!def.required && (
                    <SelectItem value={NO_REPO}>None</SelectItem>
                  )}
                  {repos.map((repo) => (
                    <SelectItem key={repo.id} value={repo.id}>
                      {repo.fullName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )
        }
        if (def.type === `pr`) {
          return (
            <div key={def.key} className="space-y-2">
              <Label>{label}</Label>
              <PrInputField
                teamId={teamId}
                value={values[def.key] ?? ``}
                required={def.required}
                onChange={(value) => onChange(def.key, value)}
              />
            </div>
          )
        }
        return (
          <div key={def.key} className="space-y-2">
            <Label>{label}</Label>
            <BoardInputField
              teamId={teamId}
              value={values[def.key] ?? ``}
              required={def.required}
              onChange={(value) => onChange(def.key, value)}
            />
          </div>
        )
      })}
    </div>
  )
}

// Open-PR single-select (EXP-259): the team's issue-linked open pull
// requests, deduped by prUrl (a batch PR links several issues to one PR —
// the row lists every linked identifier and carries the representative
// issue's id as its value). Issues don't sync team_id, so the team scope
// comes from the synced boards.
function PrInputField({
  teamId,
  value,
  required,
  onChange,
}: {
  teamId: string
  /** The picked representative issue id, or `` when unset. */
  value: string
  required: boolean
  onChange: (issueId: string) => void
}) {
  const [open, setOpen] = useState(false)

  const { data: boardRows } = useLiveQuery(
    (q) =>
      q
        .from({ boards: boardCollection })
        .where(({ boards }) => eq(boards.teamId, teamId)),
    [teamId]
  )
  const { data: issueRows } = useLiveQuery(
    (q) =>
      q
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.prState, `open`)),
    []
  )
  const pulls = useMemo(() => {
    const teamBoards = new Set(
      ((boardRows ?? []) as Board[]).map((board) => board.id)
    )
    const byPrUrl = new Map<
      string,
      { issueId: string; prNumber: number | null; identifiers: string[] }
    >()
    for (const issue of (issueRows ?? []) as Issue[]) {
      if (!teamBoards.has(issue.boardId) || !issue.prUrl) continue
      const entry = byPrUrl.get(issue.prUrl)
      if (entry) {
        entry.identifiers.push(issue.identifier)
      } else {
        byPrUrl.set(issue.prUrl, {
          issueId: issue.id,
          prNumber: issue.prNumber,
          identifiers: [issue.identifier],
        })
      }
    }
    return [...byPrUrl.values()]
      .map((entry) => ({
        ...entry,
        label: `${entry.prNumber ? `#${entry.prNumber} · ` : ``}${entry.identifiers.sort().join(`, `)}`,
      }))
      .sort((left, right) => left.label.localeCompare(right.label))
  }, [boardRows, issueRows])
  const selected = pulls.find((pull) => pull.issueId === value) ?? null

  const pick = (issueId: string) => {
    setOpen(false)
    onChange(issueId)
  }

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild>
        <Button
          variant="outline"
          className="w-full justify-start font-normal"
        >
          {selected ? (
            <span className="min-w-0 truncate">{selected.label}</span>
          ) : (
            <span className="text-muted-foreground">
              Select a pull request…
            </span>
          )}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[18rem] p-0"
        align="start"
        mobileTitle="Select a pull request"
      >
        <Command>
          <CommandInput placeholder="Select a pull request..." />
          <CommandList>
            <CommandEmpty>No open pull requests.</CommandEmpty>
            <CommandGroup>
              {!required && (
                <CommandItem value="none" onSelect={() => pick(``)}>
                  <span className="text-muted-foreground">None</span>
                  {value === `` && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              )}
              {pulls.map((pull) => (
                <CommandItem
                  key={pull.issueId}
                  // Label keeps cmdk text filtering working; the id suffix
                  // keeps values unique.
                  value={`${pull.label} ${pull.issueId}`}
                  onSelect={() => pick(pull.issueId)}
                  className="flex items-center gap-2"
                >
                  <span className="min-w-0 truncate text-sm">
                    {pull.label}
                  </span>
                  {pull.issueId === value && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}

// Board single-select over the synced boards (same MobilePopover + Command
// structure as the issue detail's move-to-board picker), with a full-width
// outline trigger to match the surrounding form fields.
function BoardInputField({
  teamId,
  value,
  required,
  onChange,
}: {
  teamId: string
  /** The picked board id, or `` when unset. */
  value: string
  required: boolean
  onChange: (boardId: string) => void
}) {
  const [open, setOpen] = useState(false)

  const { data: boardRows } = useLiveQuery(
    (q) =>
      q
        .from({ boards: boardCollection })
        .where(({ boards }) => eq(boards.teamId, teamId)),
    [teamId]
  )
  const boards = useMemo(
    () =>
      [...((boardRows ?? []) as Board[])].sort((left, right) =>
        left.name.localeCompare(right.name)
      ),
    [boardRows]
  )
  const selected = boards.find((board) => board.id === value) ?? null

  const pick = (boardId: string) => {
    setOpen(false)
    onChange(boardId)
  }

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild>
        <Button
          variant="outline"
          className="w-full justify-start font-normal"
        >
          {selected ? (
            <>
              <div
                className="h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: selected.color }}
              />
              <span className="min-w-0 truncate">{selected.name}</span>
            </>
          ) : (
            <span className="text-muted-foreground">Select a board…</span>
          )}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Select a board"
      >
        <Command>
          <CommandInput placeholder="Select a board..." />
          <CommandList>
            <CommandEmpty>No boards found.</CommandEmpty>
            <CommandGroup>
              {!required && (
                <CommandItem value="none" onSelect={() => pick(``)}>
                  <span className="text-muted-foreground">None</span>
                  {value === `` && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              )}
              {boards.map((board) => (
                <CommandItem
                  key={board.id}
                  // Name keeps cmdk text filtering working; the id suffix
                  // keeps values unique when two boards share a name.
                  value={`${board.name} ${board.id}`}
                  onSelect={() => pick(board.id)}
                  className="flex items-center gap-2"
                >
                  <div
                    className="h-2.5 w-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: board.color }}
                  />
                  <span className="min-w-0 truncate text-sm">{board.name}</span>
                  {board.id === value && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
