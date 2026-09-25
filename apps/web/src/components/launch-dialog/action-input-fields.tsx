import { useEffect, useMemo, useRef } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { type ActionInputDef, type BoardIcon } from "@exp/db-schema/domain"
import {
  Combobox,
  IconPicker,
  Label,
  GlassGroup,
  Picker,
  PickerTrigger,
  boardPickerItems,
  type PickerItem,
  type PickerOption,
  BoardGlyph,
} from "@exp/ui"
import type { Board, Issue, SyncedWorkflow } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
  workflowCollection,
} from "@/lib/collections"
import { buildPrOptions, findPrOptionForIssue } from "@/lib/pr-options"
import type { ActionRepoOption } from "@/components/action-editor-dialog"

// The selected action's typed input fields (EXP-257; EXP-825 retired the
// free-text kinds — the composer's own text is the run's instructions):
// repo → a `Combobox` row over the team's connected repos, board → the same
// picker over the synced boards, pr (EXP-259) → the same picker over the
// team's OPEN issue-linked pull requests (deduped by prUrl — a batch PR shows
// once, its value is the representative issue's id), icon (EXP-273) → the
// curated swatch grid shared with the board form. Values live in the dialog
// shell as a flat Record<key, string> — repo/board/pr store the picked id and
// icon stores the registry NAME, blank = unset (dropped from the payload by
// buildInputsPayload); EXP-941 retired the `none` sentinel these two pickers
// used for "unset" — the `Combobox` reports `null` and the mapping happens
// here.

// An empty string is no usable option identity; the unset optional repo
// rides this sentinel inside the dialog only.
const NO_REPO = `none`
// The same for an optional board input: the picker API has no `noneLabel`, so
// the clearing row is an ordinary row with a sentinel value, mapped back to
// `""` here.
const NO_BOARD = `none`

export function ActionInputFields({
  defs,
  values,
  onChange,
  repos,
  teamId,
  seedPrIssueId,
}: {
  defs: ActionInputDef[]
  values: Record<string, string>
  onChange: (key: string, value: string) => void
  /** The team's connected repos, for `repo` inputs. */
  repos: ActionRepoOption[]
  teamId: string
  /** Any issue id linked to the PR a `pr` input should open pre-picked. */
  seedPrIssueId?: string
}) {
  if (defs.length === 0) return null
  return (
    <div className="space-y-3">
      {defs.map((def) => {
        const label = def.required ? def.label : `${def.label} (optional)`
        if (def.type === `repo`) {
          // EXP-616: a pure single-select carries its label INSIDE the row
          // (the iOS grouped-form shape); the free-text fields above keep
          // their label above, where a long placeholder needs the width.
          return (
            <GlassGroup key={def.key}>
              <Combobox
                triggerVariant="row"
                searchable={false}
                mobileTitle={label}
                value={values[def.key] || (def.required ? null : NO_REPO)}
                onChange={(value) => {
                  if (value !== null) {
                    onChange(def.key, value === NO_REPO ? `` : value)
                  }
                }}
                triggerLabel="Select a repository"
                options={[
                  ...(def.required ? [] : [{ value: NO_REPO, label: `None` }]),
                  ...repos.map((repo) => ({
                    value: repo.id,
                    label: repo.fullName,
                  })),
                ]}
              />
            </GlassGroup>
          )
        }
        if (def.type === `pr`) {
          return (
            <div key={def.key} className="min-w-0 space-y-2">
              <Label>{label}</Label>
              <PrInputField
                teamId={teamId}
                value={values[def.key] ?? ``}
                required={def.required}
                seedIssueId={seedPrIssueId}
                onChange={(value) => onChange(def.key, value)}
              />
            </div>
          )
        }
        if (def.type === `icon`) {
          // EXP-273: the same curated picker the board form uses — the value
          // is a registry icon NAME, not an id, so there is nothing to scope
          // to the team.
          return (
            <div key={def.key} className="space-y-2">
              <Label>{label}</Label>
              <IconPicker
                value={(values[def.key] ?? ``) as BoardIcon | ``}
                onChange={(icon) => onChange(def.key, icon)}
                allowsNone={!def.required}
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
  seedIssueId,
  onChange,
}: {
  teamId: string
  /** The picked representative issue id, or `` when unset. */
  value: string
  required: boolean
  /**
   * ANY issue id linked to the PR this field should open pre-picked (EXP-323 —
   * a Reviews row's representative is the newest linked issue, not this list's
   * lowest-id one, so it is resolved by membership). Applied once per mount.
   */
  seedIssueId?: string
  onChange: (issueId: string) => void
}) {
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
  // EXP-1072: a workflow's open final pull request is a pickable PR too.
  const { data: workflowRows } = useLiveQuery(
    (q) =>
      q
        .from({ workflows: workflowCollection })
        .where(({ workflows }) =>
          and(eq(workflows.teamId, teamId), eq(workflows.finalPrState, `open`))
        ),
    [teamId]
  )
  const pulls = useMemo(() => {
    const teamBoards = new Set(
      ((boardRows ?? []) as Board[]).map((board) => board.id)
    )
    return buildPrOptions(
      (issueRows ?? []) as Issue[],
      teamBoards,
      ((workflowRows ?? []) as SyncedWorkflow[]).map((workflow) => ({
        id: workflow.id,
        teamId: workflow.teamId,
        name: workflow.name,
        finalPrNumber: workflow.finalPrNumber,
        // The pg enum's inferred type is wider than the wire's four words.
        finalPrState: (workflow.finalPrState as string | null) ?? null,
      }))
    )
  }, [boardRows, issueRows, workflowRows])
  const options = useMemo<PickerOption[]>(
    () =>
      pulls.map((pull) => ({ value: pull.issueId, label: pull.label })),
    [pulls]
  )

  // Seed the preselected PR once the options land (the dialog clears input
  // values on open, so the seed can't live there). The ref latch keeps a
  // manual re-pick — including clearing an optional field — from being stomped.
  const seeded = useRef(false)
  useEffect(() => {
    if (seeded.current || !seedIssueId || pulls.length === 0) return
    const option = findPrOptionForIssue(pulls, seedIssueId)
    if (!option) return
    seeded.current = true
    onChange(option.issueId)
  }, [seedIssueId, pulls, onChange])

  return (
    <Combobox
      options={options}
      value={value === `` ? null : value}
      onChange={(issueId) => onChange(issueId ?? ``)}
      noneLabel={required ? undefined : `None`}
      triggerVariant="field"
      triggerLabel="Select a pull request…"
      width="lg"
      mobileTitle="Select a pull request"
      placeholder="Select a pull request..."
      emptyText="No open pull requests."
    />
  )
}

// Board single-select over the synced boards, with a full-width field trigger
// to match the surrounding form fields.
//
// EXP-1030: the rows are THE board picker's (`boardPickerItems`, EXP-1021) —
// every board draws its icon in its colour, here and on the trigger. The
// shell is the `Picker` primitive rather than the typed `BoardPicker`
// component for one reason: an OPTIONAL board input has to be clearable, and
// no typed picker forwards a `None` row (`Combobox`'s `noneLabel` has no
// counterpart on the picker API). The extra row is the only difference — the
// board rows themselves come from the shared builder.
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
  const items = useMemo<PickerItem[]>(
    () => [
      ...(required ? [] : [{ value: NO_BOARD, label: `None` }]),
      ...boardPickerItems(boards),
    ],
    [boards, required]
  )
  const picked = boards.find((board) => board.id === value)

  return (
    <Picker
      mode="single"
      items={items}
      // Unset reads as the `None` row while there is one — the same shape the
      // repo input above uses.
      value={value === `` ? (required ? null : NO_BOARD) : value}
      onChange={(boardId) => onChange(boardId === NO_BOARD ? `` : boardId)}
      search
      width="sm"
      mobileTitle="Select a board"
      searchPlaceholder="Select a board..."
      emptyText="No boards found."
      trigger={
        <PickerTrigger
          variant="field"
          label="Select a board…"
          value={
            picked ? (
              <span className="flex min-w-0 items-center gap-2">
                <BoardGlyph board={picked} className="size-3.5 shrink-0" />
                <span className="min-w-0 truncate">{picked.name}</span>
              </span>
            ) : undefined
          }
        />
      }
    />
  )
}
