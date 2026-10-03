import { useEffect, useMemo, useState } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { useLiveQuery, eq } from "@tanstack/react-db"
import type { Board } from "@/db/schema"
import { useSession } from "@/hooks/use-session"
import { useTeamById } from "@/hooks/use-team-data"
import { boardCollection } from "@/lib/collections"
import { githubConnectedDeepLink } from "@/lib/deep-link"
import { pageTitle } from "@/lib/page-title"
import {
  GH_CLOSE_WINDOW,
  GH_CONTINUE,
  GH_INSTALLED_SIGNED_OUT_BODY,
  GH_INSTALLED_SIGNED_OUT_TITLE,
  GH_PAGE_TITLE,
  GH_RETURN_TO_APP,
  GH_SIGN_IN,
} from "@/lib/github-connect-copy"
import {
  GithubConnectFlow,
  GithubFlowDoneButton,
} from "@/components/github-connect-flow"
import { WizardFrame } from "@/components/onboarding/wizard"
import { Button, GlassGroup, IconDisc, conceptIcon } from "@exp/ui"

// SLOP-7: THE guided GitHub page — Connect GitHub → Install the app → Pick a
// repository (`components/github-connect-flow.tsx`). Three arrivals share it:
//  • a POPUP opened by a web surface (`?return=popup`): the opener re-probes
//    on focus, so the done button hands focus back and closes the popup;
//  • the SYSTEM BROWSER opened by a native (`?return=app`): the done button
//    fires `exponential://github-connected` to hand the person back;
//  • the GitHub App's Setup URL redirect after an install (no marker,
//    `?installation_id=…&setup_action=…` which is deliberately NOT read): a
//    signed-in person continues the flow in place; a signed-out one (the
//    install was started from a native, whose browser has no web session)
//    sees a plain "installed, return to the app" card with a sign-in link.
//
// Not under `_authenticated` for that last arrival. Session-aware instead:
// nothing renders until the session resolves (EXP-1132's lesson).
type ConnectSearch = {
  team?: string
  board?: string
  return?: `app` | `popup`
  link_error?: string
}

const uuid = (value: unknown): string | undefined =>
  typeof value === `string` &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)
    ? value
    : undefined

export const Route = createFileRoute(`/integrations/github`)({
  head: () => ({ meta: [{ title: pageTitle(GH_PAGE_TITLE) }] }),
  ssr: false,
  validateSearch: (search: Record<string, unknown>): ConnectSearch => ({
    team: uuid(search.team),
    board: uuid(search.board),
    return:
      search.return === `app` || search.return === `popup`
        ? search.return
        : undefined,
    link_error:
      search.link_error === undefined || search.link_error === null
        ? undefined
        : String(search.link_error),
  }),
  component: GithubConnectPage,
})

const GithubIcon = conceptIcon(`ui-github`)
const CheckIcon = conceptIcon(`ui-check`)

function GithubConnectPage() {
  const search = Route.useSearch()
  const navigate = useNavigate()
  const { data: session, isPending } = useSession()
  const signedIn = Boolean(session?.user)

  // The link round-trip returns to the page's own URL: everything but the
  // one-shot `link_error` marker, so a retry starts clean.
  const callbackURL = useMemo(() => {
    if (typeof window === `undefined`) return `/integrations/github`
    const url = new URL(window.location.href)
    url.searchParams.delete(`link_error`)
    url.searchParams.delete(`error`)
    url.searchParams.delete(`installation_id`)
    url.searchParams.delete(`setup_action`)
    return `${url.pathname}${url.search}`
  }, [])

  const team = useTeamById(signedIn ? (search.team ?? null) : null)
  const { data: boardRows } = useLiveQuery(
    (query) =>
      signedIn && search.board
        ? query
            .from({ b: boardCollection })
            .where(({ b }) => eq(b.id, search.board!))
        : undefined,
    [signedIn, search.board]
  )
  const board = (boardRows?.[0] ?? null) as Board | null

  // Only a popup (a distinct opener) can be closed; a native browser tab and
  // a plain tab get a button that does the right thing for them.
  const [isPopup] = useState(
    () =>
      typeof window !== `undefined` &&
      Boolean(window.opener) &&
      window.opener !== window
  )

  const [closed, setClosed] = useState(false)
  useEffect(() => {
    if (search.return === `app` || !isPopup) return
    // A popup arriving back from GitHub's install redirect with no return
    // marker is still the popup the app opened: keep its context.
  }, [search.return, isPopup])

  const finish = () => {
    if (search.return === `app`) {
      window.location.href = githubConnectedDeepLink()
      return
    }
    if (isPopup) {
      try {
        window.opener?.focus()
        window.close()
        setClosed(true)
      } catch {
        setClosed(true)
      }
      return
    }
    void navigate({ to: `/` })
  }

  if (isPending) return null

  if (!signedIn) {
    const here =
      typeof window === `undefined`
        ? `/integrations/github`
        : `${window.location.pathname}${window.location.search}`
    return (
      <WizardFrame>
        <GlassGroup>
          <div className="flex flex-col gap-1.5 p-6 text-center">
            <IconDisc icon={CheckIcon} tone="success" className="mx-auto" />
            <h2 className="text-xl font-semibold">{GH_INSTALLED_SIGNED_OUT_TITLE}</h2>
            <p className="text-sm text-muted-foreground">{GH_INSTALLED_SIGNED_OUT_BODY}</p>
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2 px-6 pb-6">
            <Button asChild size="lg">
              <a href={githubConnectedDeepLink()}>
                <GithubIcon className="mr-2 size-4" />
                {GH_RETURN_TO_APP}
              </a>
            </Button>
            <Button asChild variant="outline" size="lg">
              <Link to="/auth/login" search={{ redirect: here }}>
                {GH_SIGN_IN}
              </Link>
            </Button>
          </div>
        </GlassGroup>
      </WizardFrame>
    )
  }

  const doneLabel =
    search.return === `app`
      ? GH_RETURN_TO_APP
      : isPopup
        ? GH_CONTINUE
        : GH_CONTINUE

  return (
    <WizardFrame>
      <GithubConnectFlow
        teamId={team?.id ?? search.team ?? null}
        board={board ? { id: board.id, name: board.name } : null}
        callbackURL={callbackURL}
        linkError={search.link_error ?? null}
        doneAction={
          closed ? (
            <p className="text-sm text-muted-foreground">{GH_CLOSE_WINDOW}</p>
          ) : (
            <GithubFlowDoneButton onClick={finish}>{doneLabel}</GithubFlowDoneButton>
          )
        }
      />
    </WizardFrame>
  )
}
