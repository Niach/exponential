import { useMemo, type ReactNode } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  BoardGlyph,
  conceptIcon,
  EmptyState,
  GlassSectionHeader,
  PrList,
  StackRail,
  type PrListRow,
} from "@exp/ui"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import {
  useReviewsData,
  type ReviewEntry,
  type ReviewItem,
} from "@/hooks/use-reviews-data"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"
import { sessionIdentity } from "@/lib/session-identity"
import { REPO_BAND_CAPTION, RUN_BAND_CAPTION } from "@/lib/reviews-queue"

// Cross-board review queue: every open PR in the team, banded by board, then
// the run PRs and the unlinked GitHub PRs. EXP-1248: ONE line per PR
// (`PrRow` ×4): a PR TREE nests with tree guides, a linear STACK hangs off
// one rail down to its base branch, singles sit flat. The list only OPENS:
// merging lives on the Guide's merge control, never on a row.
export const Route = createFileRoute(`/t/$teamSlug/reviews/`)({
  head: () => ({ meta: [{ title: pageTitle(`Reviews`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ReviewsPage,
})

// EXP-916: Reviews is a ×4 surface, so its glyphs are CONCEPTS.
const PrOpenIcon = conceptIcon(`pr-open`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

/** The quiet word on a stack's top row ×4. */
export const STACK_WORD = `stack`
/** The quiet word on an unlinked draft PR. */
export const DRAFT_WORD = `draft`

/**
 * A board-band row's mono label + title: the issue's identifier and title;
 * a batch PR names its first issue `EXP-874 +2` (the batch-run convention).
 */
export function reviewRowLabel(entry: Pick<ReviewEntry, `issues`>): {
  identifier: string
  title: string
} {
  const first = entry.issues[0]!
  const more = entry.issues.length - 1
  return {
    identifier: more > 0 ? `${first.identifier} +${more}` : first.identifier,
    title: first.title,
  }
}

/** Board-band items as drawn: consecutive `pr` items share ONE `PrList` (so a
 *  tree's guides span its rows), each stack is its own rail. */
export function reviewBlocks(
  items: readonly ReviewItem[]
): Array<
  | { kind: `list`; rows: Array<{ entry: ReviewEntry; depth: number }> }
  | { kind: `stack`; entries: ReviewEntry[]; baseBranch: string | null }
> {
  const blocks: ReturnType<typeof reviewBlocks> = []
  for (const item of items) {
    if (item.kind === `stack`) {
      blocks.push({ kind: `stack`, entries: item.entries, baseBranch: item.baseBranch })
      continue
    }
    const last = blocks[blocks.length - 1]
    const row = { entry: item.entry, depth: item.depth }
    if (last?.kind === `list`) last.rows.push(row)
    else blocks.push({ kind: `list`, rows: [row] })
  }
  return blocks
}

function ReviewsPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  // EXP-1186: the phone's Reviews reads EVERY member team, like the inbox;
  // with more than one, each band names its team. md+ = the active team.
  const scope = useCrossTeamScope(team)
  const { groups, sessionGroups, externalGroups, count, isLoading, externalLoading } =
    useReviewsData(team, scope.teams, { force: true })
  const teamById = useMemo(
    () => new Map(scope.teams.map((row) => [row.id, row])),
    [scope.teams]
  )
  // A band's quiet team caption — only while the list spans several teams.
  const teamCaption = (teamId: string | undefined) => {
    const name = scope.grouped && teamId ? teamById.get(teamId)?.name : undefined
    return name ? (
      <span className="truncate text-xs text-muted-foreground">{name}</span>
    ) : undefined
  }

  // A row opens the issue's Guide under the row's OWN team (EXP-1186);
  // `from=reviews` keeps the queue as the Back target (EXP-851).
  const openIssue = (rowTeamSlug: string, boardSlug: string, issueIdentifier: string) => {
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: rowTeamSlug, boardSlug, issueIdentifier },
      search: { from: `reviews`, view: `guide` },
    })
  }
  // EXP-1194: a run's own PR has no issue: it opens the RUN's Guide.
  const openRun = (rowTeamSlug: string, sessionId: string) => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: rowTeamSlug, sessionId },
      search: { from: `reviews`, view: `guide` },
    })
  }

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  const band = (key: string, header: ReactNode, body: ReactNode) => (
    <section key={key} className="mb-6" data-testid={`review-band-${key}`}>
      {header}
      <div className="mt-1 flex flex-col">{body}</div>
    </section>
  )

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it.
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 ${TAB_BAR_CLEARANCE}`}>
        {isLoading ? (
          <div className="text-muted-foreground px-1 py-6 text-sm">Loading…</div>
        ) : count === 0 ? (
          externalLoading ? (
            <div className="text-muted-foreground px-1 py-6 text-sm">Loading…</div>
          ) : (
            <EmptyState
              icon={PrOpenIcon}
              title="No open pull requests"
              description={
                scope.teams.length > 1
                  ? `Open pull requests in your teams' repositories land here for review.`
                  : `Open pull requests in this team's repositories land here for review.`
              }
            />
          )
        ) : (
          <>
            {groups.map((group) => {
              const rowTeam = group.team ?? team
              const open = (entry: ReviewEntry) => () =>
                openIssue(rowTeam.slug, group.board.slug, entry.issue.identifier)
              return band(
                group.board.id,
                // EXP-1248: list bands carry NO count.
                <GlassSectionHeader
                  leading={<BoardGlyph board={group.board} className="size-3.5" />}
                  label={group.board.name}
                  trailing={teamCaption(group.board.teamId)}
                />,
                reviewBlocks(group.items).map((block, index) =>
                  block.kind === `stack` ? (
                    <StackRail
                      key={`stack:${block.entries[0]?.key ?? index}`}
                      members={block.entries.map((entry) => ({
                        key: entry.key,
                        entry,
                        ...reviewRowLabel(entry),
                      }))}
                      baseBranch={
                        block.baseBranch ?? group.board.defaultBranch ?? `default branch`
                      }
                      word={STACK_WORD}
                      onOpen={(member) => open(member.entry)()}
                    />
                  ) : (
                    <PrList
                      key={`list:${block.rows[0]?.entry.key ?? index}`}
                      rows={block.rows.map(
                        ({ entry, depth }): PrListRow => ({
                          key: entry.key,
                          ...reviewRowLabel(entry),
                          depth,
                          onOpen: open(entry),
                        })
                      )}
                    />
                  )
                )
              )
            })}

            {/* EXP-734: PRs a coding run opened for itself (an action or
                chat run with no linked issue). EXP-1186: one band per team. */}
            {sessionGroups.map((sessionGroup) =>
              sessionGroup.entries.length === 0
                ? null
                : band(
                    `runs:${sessionGroup.team?.id ?? `runs`}`,
                    <GlassSectionHeader
                      leading={
                        <PrOpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
                      }
                      label="Agent runs"
                      trailing={
                        teamCaption(sessionGroup.team?.id) ?? (
                          <span className="text-xs text-muted-foreground">
                            {RUN_BAND_CAPTION}
                          </span>
                        )
                      }
                    />,
                    <PrList
                      rows={sessionGroup.entries.map(({ key, session }) => ({
                        key,
                        identifier: session.prNumber ? `#${session.prNumber}` : null,
                        // EXP-1204: the run's own name ×4.
                        title: sessionIdentity({ session, issue: undefined }).subject,
                        onOpen: () =>
                          openRun(sessionGroup.team?.slug ?? teamSlug, session.id),
                      }))}
                    />
                  )
            )}

            {/* Unlinked GitHub PRs: grouped by repository, they open on
                GitHub (the muted external-link glyph says so). */}
            {externalGroups.map((group) =>
              band(
                `repo:${group.teamId}:${group.repositoryId}`,
                <GlassSectionHeader
                  leading={
                    <PrOpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  }
                  label={group.fullName}
                  trailing={
                    teamCaption(group.teamId) ?? (
                      <span className="text-xs text-muted-foreground">
                        {REPO_BAND_CAPTION}
                      </span>
                    )
                  }
                />,
                <PrList
                  rows={group.pulls.map((pull) => ({
                    key: String(pull.number),
                    identifier: `#${pull.number}`,
                    title: pull.title,
                    word: pull.draft ? DRAFT_WORD : null,
                    onOpen: () => {
                      window.open(pull.url, `_blank`, `noopener,noreferrer`)
                    },
                    trailing: (
                      <ExternalLinkIcon
                        aria-hidden
                        className="size-3.5 shrink-0 text-muted-foreground"
                      />
                    ),
                  }))}
                />
              )
            )}
          </>
        )}
      </div>
    </div>
  )
}
