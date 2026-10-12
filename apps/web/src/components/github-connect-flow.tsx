import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
  startGithubLink,
} from "@/lib/github-connect"
import {
  GH_CONNECT_GITHUB,
  GH_DONE_BODY_NO_BOARD,
  GH_DONE_TITLE,
  GH_INSTALL_ANOTHER,
  GH_INSTALL_APP,
  GH_LINK_FAILED,
  GH_PAGE_INTRO,
  GH_PAGE_TITLE,
  GH_RECONNECT_GITHUB,
  GH_RECONNECT_NEEDED,
  GH_STEP_CONNECT_BODY,
  GH_STEP_CONNECT_MET,
  GH_STEP_CONNECT_TITLE,
  GH_STEP_INSTALL_BODY,
  GH_STEP_INSTALL_MET,
  GH_STEP_INSTALL_TITLE,
  GH_STEP_REPO_BODY,
  GH_STEP_REPO_MET,
  GH_STEP_REPO_TITLE,
  ghDoneBody,
  ghStepRepoBodyForBoard,
  githubInstallationLabel,
} from "@/lib/github-connect-copy"
import {
  GithubRepoPicker,
  type PickerRepo,
  type ReposResult,
} from "@/components/github-repo-picker"
import {
  Button,
  GlassGroup,
  IconDisc,
  ReadinessFixPill,
  ReadinessFixes,
  ReadinessProgress,
  ReadinessRow,
  ReadinessRows,
  conceptIcon,
  type ReadinessRowState,
} from "@exp/ui"

// SLOP-7: the ONE guided GitHub flow — three rows in the readiness
// checklist's language (met / current / pending, the current row carrying
// its fix), then a done state:
//
//   1. Connect your GitHub account   → Better Auth linkSocial (full-page
//                                      redirect through GitHub, back here)
//   2. Install the Exponential app   → GitHub's install page (same window:
//                                      the App's Setup URL redirects back)
//   3. Pick a repository             → the live picker; the pick adds the
//                                      repo to the team and, with a board
//                                      in context, points the board at it
//
// Hosted by `/integrations/github` (a popup over any web surface, the
// system browser for a native, the App's own setup redirect). The flow reads
// `integrations.github.repos` for everything (linked, token health,
// installations, repos) and re-reads on focus, so a step done on GitHub
// ticks off the moment the person is back.

const GithubIcon = conceptIcon(`ui-github`)
const InstallIcon = conceptIcon(`ui-download`)
const RepoIcon = conceptIcon(`ui-branch`)
const CheckIcon = conceptIcon(`ui-check`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

export interface GithubConnectFlowProps {
  /** The team the picked repository is added to; null = stop after the
   * install step (the pickers in the app list repositories themselves). */
  teamId: string | null
  /** The board the picked repository is set on (`boards.setRepository`). */
  board: { id: string; name: string } | null
  /** Where the link round-trip lands (this page's own URL). */
  callbackURL: string
  /** Arrived with `?link_error=` — the link round-trip failed. */
  linkError?: string | null
  /** The done state's action (close the popup, deep-link, navigate). */
  doneAction: React.ReactNode
  /** Fired once when the flow reaches its done state. */
  onDone?: () => void
}

type StepKey = `connect` | `install` | `repo`

export function GithubConnectFlow({
  teamId,
  board,
  callbackURL,
  linkError,
  doneAction,
  onDone,
}: GithubConnectFlowProps) {
  const [data, setData] = useState<ReposResult | null>(null)
  const [failed, setFailed] = useState(false)
  const [busy, setBusy] = useState(false)
  const [added, setAdded] = useState<string | null>(null)

  // Without a team the page cannot list (every integrations query is
  // team-scoped); the person's default team stands in for the connect and
  // install steps.
  const [resolvedTeamId, setResolvedTeamId] = useState<string | null>(teamId)
  useEffect(() => {
    if (teamId) {
      setResolvedTeamId(teamId)
      return
    }
    let live = true
    trpc.teams.getDefault
      .query()
      .then(({ team }) => {
        if (live) setResolvedTeamId(team?.id ?? null)
      })
      .catch(() => {
        // FEED-79: a load on open never toasts; the steps stay as they are.
        if (live) setFailed(true)
      })
    return () => {
      live = false
    }
  }, [teamId])

  // FEED-79: the list runs on open and on every window focus, so a failure
  // stays quiet (no toast): the steps keep their last known state.
  const refresh = useCallback(
    async (force = false) => {
      if (!resolvedTeamId) return
      try {
        setData(
          await trpc.integrations.github.repos.query({
            teamId: resolvedTeamId,
            ...(force ? { refresh: true } : {}),
          })
        )
        setFailed(false)
      } catch {
        setFailed(true)
      }
    },
    [resolvedTeamId]
  )

  useEffect(() => {
    void refresh(true)
  }, [refresh])

  // Back from GitHub's install page in another tab, or from the popup's
  // own round-trip: re-list.
  useEffect(() => {
    const onFocus = () => void refresh(true)
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [refresh])

  const connect = async () => {
    setBusy(true)
    const message = await startGithubLink(
      callbackURL,
      `${callbackURL}${callbackURL.includes(`?`) ? `&` : `?`}link_error=1`
    )
    if (message) {
      failedAction(`connect`, `GitHub`, message, { retry: () => void connect() })
      setBusy(false)
    }
    // On success the browser is navigating to GitHub.
  }

  // Back from GitHub's error callback (`link_error=1`): the connect failed.
  // Toast it once on arrival; Retry starts the link again.
  const linkErrorShown = useRef(false)
  useEffect(() => {
    if (!linkError || linkErrorShown.current) return
    linkErrorShown.current = true
    failedAction(`connect`, `GitHub`, GH_LINK_FAILED, { retry: () => void connect() })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linkError])

  // Same window: GitHub's setup URL brings the person back to this page.
  const install = () => {
    if (!data?.installUrl) return
    window.location.assign(data.installUrl)
  }

  // A blocked popup: the toast's Retry re-opens it (a fresh user gesture).
  const installAnother = () => {
    if (openGithubPopup(data?.installUrl)) return
    failedAction(`install`, `the GitHub app`, POPUP_BLOCKED_MESSAGE, { retry: installAnother })
  }

  const pick = async (repo: PickerRepo) => {
    if (!resolvedTeamId || busy) return
    setBusy(true)
    try {
      const { repository } = await trpc.repositories.add.mutate(
        {
          teamId: resolvedTeamId,
          fullName: repo.fullName,
          defaultBranch: repo.defaultBranch,
          private: repo.private,
        },
        SKIP_ERROR_TOAST
      )
      if (board && repository) {
        await trpc.boards.setRepository.mutate(
          { boardId: board.id, repositoryId: repository.id },
          SKIP_ERROR_TOAST
        )
      }
      setAdded(repo.fullName)
    } catch (err) {
      // Both writes are idempotent (add registers once, the board points
      // at the same row), so Retry is safe.
      failedAction(`add`, repo.fullName, err, { retry: () => void pick(repo) })
    } finally {
      setBusy(false)
    }
  }

  const linked = Boolean(data?.linked) && !data?.needsReconnect
  const installed = Boolean(data?.installed)
  const states = useMemo<Record<StepKey, ReadinessRowState>>(() => {
    if (added) return { connect: `met`, install: `met`, repo: `met` }
    if (!linked) return { connect: `current`, install: `pending`, repo: `pending` }
    if (!installed) return { connect: `met`, install: `current`, repo: `pending` }
    return { connect: `met`, install: `met`, repo: `current` }
  }, [added, linked, installed])

  const loading = data === null && !failed
  const configured = data?.configured ?? true
  const done = added !== null || (teamId === null && installed)
  // The host decides what "done" means for its arrival (close the popup, go
  // back where the person came from); it is told once, when done flips.
  useEffect(() => {
    if (done) onDone?.()
  }, [done, onDone])

  return (
    <GlassGroup className="w-full" data-testid="github-connect-flow">
      <div className="flex flex-col gap-1.5 p-6 text-center">
        <IconDisc icon={done ? CheckIcon : GithubIcon} className="mx-auto" tone={done ? `success` : `primary`} />
        <h2 className="text-xl font-semibold">{done ? GH_DONE_TITLE : GH_PAGE_TITLE}</h2>
        <p className="text-sm text-muted-foreground">
          {done
            ? added
              ? ghDoneBody(added)
              : GH_DONE_BODY_NO_BOARD
            : GH_PAGE_INTRO}
        </p>
        {!done && !loading && configured && (
          <ReadinessProgress
            className="mt-2"
            states={[states.connect, states.install, states.repo]}
          />
        )}
      </div>

      {!configured && (
        <p className="border-t border-glass-stroke px-6 py-4 text-sm text-muted-foreground">
          GitHub isn’t configured on this server.
        </p>
      )}

      {configured && !done && (
        <ReadinessRows className="border-t border-glass-stroke">
          <ReadinessRow
            stepKey="connect"
            icon={GithubIcon}
            state={states.connect}
            title={states.connect === `met` ? GH_STEP_CONNECT_MET : GH_STEP_CONNECT_TITLE}
            body={
              states.connect === `met`
                ? null
                : data?.needsReconnect
                  ? `${GH_RECONNECT_NEEDED} ${GH_STEP_CONNECT_BODY}`
                  : GH_STEP_CONNECT_BODY
            }
            detail={states.connect === `met` ? data?.login ?? null : null}
          >
            {states.connect === `current` && !loading && (
              <ReadinessFixes>
                <ReadinessFixPill
                  primary
                  icon={GithubIcon}
                  disabled={busy}
                  onClick={() => void connect()}
                  data-testid="github-flow-connect"
                >
                  {data?.needsReconnect ? GH_RECONNECT_GITHUB : GH_CONNECT_GITHUB}
                </ReadinessFixPill>
              </ReadinessFixes>
            )}
          </ReadinessRow>

          <ReadinessRow
            stepKey="install"
            icon={InstallIcon}
            state={states.install}
            title={states.install === `met` ? GH_STEP_INSTALL_MET : GH_STEP_INSTALL_TITLE}
            body={states.install === `met` ? null : GH_STEP_INSTALL_BODY}
            detail={
              states.install === `met`
                ? (data?.installations ?? [])
                    .map((inst) => githubInstallationLabel(inst))
                    .join(`, `) || null
                : null
            }
          >
            {states.install === `current` && (
              <ReadinessFixes>
                <ReadinessFixPill
                  primary
                  icon={ExternalLinkIcon}
                  disabled={!data?.installUrl}
                  onClick={install}
                  data-testid="github-flow-install"
                >
                  {GH_INSTALL_APP}
                </ReadinessFixPill>
              </ReadinessFixes>
            )}
          </ReadinessRow>

          <ReadinessRow
            stepKey="repo"
            icon={RepoIcon}
            state={states.repo}
            title={states.repo === `met` ? GH_STEP_REPO_MET : GH_STEP_REPO_TITLE}
            body={
              states.repo === `met`
                ? null
                : board
                  ? ghStepRepoBodyForBoard(board.name)
                  : GH_STEP_REPO_BODY
            }
          >
            {states.repo === `current` && resolvedTeamId && teamId && (
              <div className="mt-2">
                <GithubRepoPicker
                  teamId={resolvedTeamId}
                  variant="plain"
                  listClassName="max-h-64"
                  onSelect={(repo) => void pick(repo)}
                />
              </div>
            )}
            {states.repo === `current` && !teamId && (
              <ReadinessFixes>
                <ReadinessFixPill icon={ExternalLinkIcon} onClick={installAnother}>
                  {GH_INSTALL_ANOTHER}
                </ReadinessFixPill>
              </ReadinessFixes>
            )}
          </ReadinessRow>
        </ReadinessRows>
      )}

      {done && <div className="flex justify-center p-6">{doneAction}</div>}
    </GlassGroup>
  )
}

/** The done state's plain button, for hosts with nothing special to do. */
export function GithubFlowDoneButton({
  onClick,
  children,
}: {
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Button onClick={onClick}>{children}</Button>
  )
}
