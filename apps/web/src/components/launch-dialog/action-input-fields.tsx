import { useEffect, useMemo, useRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { type ActionInputDef, type BoardIcon } from "@exp/db-schema/domain"
import {
  IconPicker,
  Label,
  GlassGroup,
  Picker,
  RepositoryPicker,
  PickerTrigger,
  boardPickerItems,
  type PickerItem,
  BoardGlyph,
} from "@exp/ui"
import type { Board, Issue } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
} from "@/lib/collections"
import {
  buildPrOptions,
  findPrOptionForIssue,
  type PrOption,
} from "@/lib/pr-options"
import type { ActionRepoOption } from "@/components/action-prompt-form"

// The selected action's typed input fields (EXP-257; EXP-825 retired the
// free-text kinds — the composer's own text is the run's instructions):
// repo → a `Picker` row over the team's connected repos, board → the same
// picker over the synced boards, pr (EXP-259) → the same picker over the
// team's OPEN issue-linked pull requests (deduped by prUrl — a batch PR shows
// once, its value is the representative issue's id), icon (EXP-273) → the
// curated swatch grid shared with the board form. Values live in the dialog
// shell as a flat Record<key, string> — repo/board/pr store the picked id and
// icon stores the registry NAME, blank = unset (dropped from the payload by
// buildInputsPayload); EXP-941 retired the `none` sentinel these two pickers
// used for "unset" — the picker's none row reports it and the mapping happens
// here.

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
              <RepositoryPicker
                triggerVariant="row"
                mobileTitle={label}
                value={values[def.key] || null}
                onChange={(value) => onChange(def.key, value)}
                {...(def.required
                  ? {}
                  : { noneLabel: `None`, onNone: () => onChange(def.key, ``) })}
                triggerLabel={def.required ? `Select a repository` : `None`}
                repositories={repos.map((repo) => ({
                  id: repo.id,
                  fullName: repo.fullName,
                }))}
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
//
// EXP-1233: the OPTIONS and the seed latch are hooks of their own, because
// the Fix merge conflicts card (`fix-conflicts-card.tsx`) draws the same
// picker inside its own row — one list, two triggers.

/** The team's open pull requests as picker options, off the synced rows. */
export function useOpenPrOptions(teamId: string): PrOption[] {
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
  return useMemo(() => {
    const teamBoards = new Set(
      ((boardRows ?? []) as Board[]).map((board) => board.id)
    )
    return buildPrOptions((issueRows ?? []) as Issue[], teamBoards)
  }, [boardRows, issueRows])
}

/**
 * Seed the preselected PR once the options land (the dialog clears input
 * values on open, so the seed can't live there). The ref latch keeps a
 * manual re-pick — including clearing an optional field — from being stomped.
 * `seedIssueId` is ANY issue linked to the PR (EXP-323 — a Reviews row's
 * representative is the newest linked issue, not this list's lowest-id one,
 * so it is resolved by membership). Applied once per mount.
 */
export function usePrInputSeed(
  pulls: PrOption[],
  seedIssueId: string | undefined,
  onChange: (issueId: string) => void
) {
  const seeded = useRef(false)
  useEffect(() => {
    if (seeded.current || !seedIssueId || pulls.length === 0) return
    const option = findPrOptionForIssue(pulls, seedIssueId)
    if (!option) return
    seeded.current = true
    onChange(option.issueId)
  }, [seedIssueId, pulls, onChange])
}

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
  seedIssueId?: string
  onChange: (issueId: string) => void
}) {
  const pulls = useOpenPrOptions(teamId)
  const options = useMemo<PickerItem[]>(
    () =>
      pulls.map((pull) => ({ value: pull.issueId, label: pull.label })),
    [pulls]
  )
  usePrInputSeed(pulls, seedIssueId, onChange)

  return (
    <Picker
      mode="single"
      search
      items={options}
      value={value === `` ? null : value}
      onChange={onChange}
      noneLabel={required ? undefined : `None`}
      onNone={() => onChange(``)}
      triggerVariant="field"
      triggerLabel="Select a pull request…"
      width="lg"
      mobileTitle="Select a pull request"
      searchPlaceholder="Select a pull request..."
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
// component for one reason: an OPTIONAL board input has to be clearable,
// which is the primitive's `noneLabel` row. The board rows themselves come
// from the shared builder.
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
  const items = useMemo<PickerItem[]>(() => boardPickerItems(boards), [boards])
  const picked = boards.find((board) => board.id === value)

  return (
    <Picker
      mode="single"
      items={items}
      // Unset reads as the `None` row while there is one — the same shape the
      // repo input above uses.
      value={value === `` ? null : value}
      onChange={onChange}
      {...(required ? {} : { noneLabel: `None`, onNone: () => onChange(``) })}
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
