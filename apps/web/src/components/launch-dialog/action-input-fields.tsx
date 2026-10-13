import { useEffect, useMemo, useRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { type ActionInputDef } from "@exp/db-schema/domain"
import {
  Label,
  GlassGroup,
  Picker,
  RepositoryPicker,
  PickerTrigger,
  boardPickerItems,
  type PickerItem,
  BoardGlyph,
} from "@exp/ui"
import type { Board } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
  pullRequestCollection,
  pullRequestIssueCollection,
} from "@/lib/collections"
import { buildPrOptions, findPrOption, type PrOption } from "@/lib/pr-options"

// The selected action's typed input fields (EXP-257; EXP-825 retired the
// free-text kinds: the composer's own text is the session's instructions).
// Every input is a PICK, repo | board | pr | issue (EXP-1293 dropped `icon`
// with the Create action builtin): repo → a `Picker` row over the team's
// connected repos, board → the same picker over the synced boards, pr → the
// team's OPEN pull requests off the synced `pull_requests` rows (the value is
// the pull request's id), issue → the team's issues. Values are a flat
// Record<key, string> of picked ids, blank = unset (dropped from the payload
// by buildInputsPayload).

/** A repository the `repo` inputs offer. */
export interface ActionInputRepo {
  id: string
  fullName: string
}

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
  repos: ActionInputRepo[]
  teamId: string
  /** The PR (or any issue it closes) a `pr` input opens pre-picked on. */
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
        if (def.type === `issue`) {
          return (
            <div key={def.key} className="space-y-2">
              <Label>{label}</Label>
              <IssueInputField
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

// Open-PR single-select (EXP-259): the team's open pull requests off the
// synced `pull_requests` rows, labelled by the issues they close
// (`pull_request_issues`). Never read off the issues (EXP-1293: PR state
// left them).

/** The team's open pull requests as picker options, off the synced rows. */
export function useOpenPrOptions(teamId: string): PrOption[] {
  const { data: pullRows } = useLiveQuery(
    (q) =>
      q
        .from({ pulls: pullRequestCollection })
        .where(({ pulls }) => eq(pulls.teamId, teamId)),
    [teamId]
  )
  const { data: linkRows } = useLiveQuery(
    (q) =>
      q
        .from({ links: pullRequestIssueCollection })
        .where(({ links }) => eq(links.teamId, teamId)),
    [teamId]
  )
  const { data: issueRows } = useLiveQuery(
    (q) =>
      q
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.teamId, teamId))
        .select(({ issues }) => ({ id: issues.id, identifier: issues.identifier })),
    [teamId]
  )
  return useMemo(
    () =>
      buildPrOptions(
        pullRows ?? [],
        linkRows ?? [],
        new Map((issueRows ?? []).map((row) => [row.id, row.identifier])),
        teamId
      ),
    [pullRows, linkRows, issueRows, teamId]
  )
}

/**
 * Seed the preselected PR once the options land. `seedId` = the pull
 * request's id or ANY issue it closes (`findPrOption`). The ref latch keeps
 * a manual re-pick (including clearing an optional field) from being
 * stomped; applied once per mount.
 */
export function usePrInputSeed(
  pulls: PrOption[],
  seedId: string | undefined,
  onChange: (pullRequestId: string) => void
) {
  const seeded = useRef(false)
  useEffect(() => {
    if (seeded.current || !seedId || pulls.length === 0) return
    const option = findPrOption(pulls, seedId)
    if (!option) return
    seeded.current = true
    onChange(option.pullRequestId)
  }, [seedId, pulls, onChange])
}

function PrInputField({
  teamId,
  value,
  required,
  seedIssueId,
  onChange,
}: {
  teamId: string
  /** The picked pull request id, or `` when unset. */
  value: string
  required: boolean
  seedIssueId?: string
  onChange: (pullRequestId: string) => void
}) {
  const pulls = useOpenPrOptions(teamId)
  const options = useMemo<PickerItem[]>(
    () => pulls.map((pull) => ({ value: pull.pullRequestId, label: pull.label })),
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

// Issue single-select over the team's synced issues (identifier + title),
// newest first.
function IssueInputField({
  teamId,
  value,
  required,
  onChange,
}: {
  teamId: string
  /** The picked issue id, or `` when unset. */
  value: string
  required: boolean
  onChange: (issueId: string) => void
}) {
  const { data: issueRows } = useLiveQuery(
    (q) =>
      q
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.teamId, teamId))
        .select(({ issues }) => ({
          id: issues.id,
          identifier: issues.identifier,
          title: issues.title,
          createdAt: issues.createdAt,
        })),
    [teamId]
  )
  const items = useMemo<PickerItem[]>(
    () =>
      [...(issueRows ?? [])]
        .sort(
          (left, right) =>
            new Date(right.createdAt).getTime() - new Date(left.createdAt).getTime()
        )
        .map((row) => ({ value: row.id, label: `${row.identifier} ${row.title}` })),
    [issueRows]
  )
  return (
    <Picker
      mode="single"
      search
      items={items}
      value={value === `` ? null : value}
      onChange={onChange}
      noneLabel={required ? undefined : `None`}
      onNone={() => onChange(``)}
      triggerVariant="field"
      triggerLabel="Select an issue…"
      width="lg"
      mobileTitle="Select an issue"
      searchPlaceholder="Select an issue..."
      emptyText="No issues found."
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
