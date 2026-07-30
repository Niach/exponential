import { useCallback, useEffect, useState } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { CircleAlert, Building2, Check, Github, User } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { trpc } from "@/lib/trpc-client"
import { githubConnectedDeepLink } from "@/lib/deep-link"

// The OAuth claim flow's account manager: the callback verified (via GitHub's
// /user/installations) which App installations the user controls; when there
// are several, it lands here with a signed ticket and the user picks which
// GitHub accounts stay connected to the team — checking links, unchecking
// disconnects (blocked while the account still has connected repos). The
// page never auto-redirects (EXP-370): the primary button is always enabled —
// "Save" when the selection changed, otherwise a plain hand-back matching the
// arrival mode (native tab → exponential:// return, desktop popup → close
// into the opener, plain tab → Continue link). Also the shared landing page
// for the claim flow's error states (?error=…).
interface ClaimSearch {
  ticket?: string
  error?: string
  login?: string
  install?: string
}

const MOBILE_DEEP_LINK = githubConnectedDeepLink()

const ERROR_COPY: Record<string, { title: string; body: string }> = {
  session: {
    title: `Sign-in didn't match`,
    body: `This connect link expired or was started from a different session. Go back to team settings → Repositories and click Connect again.`,
  },
  exchange: {
    title: `GitHub authorization failed`,
    body: `GitHub didn't complete the authorization. Restart the connect flow from team settings → Repositories.`,
  },
  none: {
    title: `No installation found`,
    body: `That GitHub account has no installation of the Exponential App yet. Install it on your account or organization — you'll pick exactly which repositories it can see.`,
  },
  notowner: {
    title: `Not your installation`,
    body: `The GitHub account you authorized doesn't own an installation of the Exponential App — it only has collaborator access to someone else's. Install the App on your own account or organization, then connect again.`,
  },
  orgperm: {
    title: `Organization approval needed`,
    body: `Your organization hasn't approved the App's latest permissions yet (read-only organization members, used to verify your membership). Ask an org admin to approve the pending permission request on GitHub, then connect again.`,
  },
  forbidden: {
    title: `Not allowed`,
    body: `Only team owners can connect GitHub accounts to this team.`,
  },
}

// Errors where the way out is GitHub's install page (account + repo
// selection) — these render the signed `install` link as the primary action.
const INSTALLABLE_ERRORS = new Set([`none`, `notowner`, `orgperm`])

// The install param is forgeable — only ever link to the App's own install
// page on github.com. Parse instead of prefix-matching the raw string:
// dot-segments ("https://github.com/apps/../…") normalize past a startsWith
// check, so validate origin + normalized pathname and link the normalized URL.
function safeGithubInstallUrl(raw: string): string | null {
  try {
    const url = new URL(raw)
    if (url.origin !== `https://github.com`) return null
    if (!url.pathname.startsWith(`/apps/`)) return null
    return url.href
  } catch {
    return null
  }
}

interface PreviewInstallation {
  installationId: number
  accountLogin: string | null
  accountType: string | null
  alreadyLinked: boolean
  activeRepoCount: number
}

function GithubClaim() {
  const { ticket, error, login, install } = Route.useSearch()
  const installUrl = install ? safeGithubInstallUrl(install) : null
  const [preview, setPreview] = useState<{
    teamId: string
    mobile: boolean
    dialog: boolean
    installations: PreviewInstallation[]
  } | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)
  // The DESIRED linked set — checked rows. Initialized to every verified
  // installation (linked stay checked, new ones preselected — the common
  // case is "connect what I just authorized").
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [saving, setSaving] = useState(false)
  const [done, setDone] = useState(false)

  const [isPopup] = useState(
    () =>
      typeof window !== `undefined` &&
      Boolean(window.opener) &&
      window.opener !== window
  )

  const loadPreview = useCallback(async () => {
    if (!ticket) return
    try {
      const data = await trpc.integrations.github.claimPreview.query({
        ticket,
      })
      setPreview(data)
      setSelected(
        new Set(data.installations.map((i) => i.installationId))
      )
    } catch (err: unknown) {
      setLoadError(
        err instanceof Error ? err.message : `This claim link is invalid.`
      )
    }
  }, [ticket])

  useEffect(() => {
    void loadPreview()
  }, [loadPreview])

  // Hand the user back after an explicit save, matching the arrival mode.
  // Never fires on load — arriving with everything already linked shows the
  // picker with a plain "Continue"/"Back to app" button instead (EXP-370).
  useEffect(() => {
    if (!done) return
    if (preview?.mobile) {
      window.location.href = MOBILE_DEEP_LINK
      return
    }
    if (!isPopup) return
    const timer = setTimeout(() => {
      try {
        window.opener?.focus()
        window.close()
      } catch {
        // Blocked by the browser — the Continue button below is the fallback.
      }
    }, 600)
    return () => clearTimeout(timer)
  }, [done, isPopup, preview?.mobile])

  const errorCopy = error
    ? (ERROR_COPY[error] ?? ERROR_COPY.exchange)
    : loadError
      ? { title: `Claim link invalid`, body: loadError }
      : null

  const linkedIds = new Set(
    preview?.installations
      .filter((i) => i.alreadyLinked)
      .map((i) => i.installationId) ?? []
  )
  const toLink = [...selected].filter((id) => !linkedIds.has(id))
  const toUnlink = [...linkedIds].filter((id) => !selected.has(id))
  const hasChanges = toLink.length > 0 || toUnlink.length > 0

  async function save() {
    if (!ticket) return
    setSaving(true)
    setSaveError(null)
    try {
      await trpc.integrations.github.claimLinks.mutate({
        ticket,
        linkIds: toLink,
        unlinkIds: toUnlink,
      })
      setDone(true)
    } catch (err) {
      setSaveError(
        err instanceof Error ? err.message : `Saving failed — try again.`
      )
      // Repos may have been connected meanwhile (CONFLICT) — refresh so the
      // affected row re-disables with its in-use note. A stale ticket fails
      // the refresh into the full error card, which is the right dead end.
      void loadPreview()
    } finally {
      setSaving(false)
    }
  }

  function closePopup() {
    try {
      window.opener?.focus()
      window.close()
    } catch {
      // Blocked — nothing else to do, the tab stays open.
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4 sm:p-6">
      <Card className="w-full max-w-md">
        {errorCopy ? (
          <>
            <CardHeader className="flex flex-col items-center gap-3 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-red-500/15 text-red-500">
                <CircleAlert className="h-6 w-6" strokeWidth={2.5} />
              </div>
              <CardTitle className="text-xl">{errorCopy.title}</CardTitle>
              <CardDescription>{errorCopy.body}</CardDescription>
              {login ? (
                <p className="text-xs text-muted-foreground">
                  You authorized GitHub as <span className="font-medium">{login}</span>.
                </p>
              ) : null}
            </CardHeader>
            <CardContent className="flex flex-col gap-2">
              {installUrl && error && INSTALLABLE_ERRORS.has(error) ? (
                <>
                  <Button asChild size="lg" className="w-full">
                    <a href={installUrl}>
                      <Github className="h-4 w-4" />
                      Install on GitHub
                    </a>
                  </Button>
                  <Button asChild size="lg" variant="ghost" className="w-full">
                    <Link to="/">Back to Exponential</Link>
                  </Button>
                </>
              ) : (
                <Button asChild size="lg" className="w-full">
                  <Link to="/">Back to Exponential</Link>
                </Button>
              )}
            </CardContent>
          </>
        ) : done ? (
          <>
            <CardHeader className="flex flex-col items-center gap-3 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-500">
                <Check className="h-6 w-6" strokeWidth={2.5} />
              </div>
              <CardTitle className="text-xl">GitHub connections saved</CardTitle>
              <CardDescription>
                {preview?.mobile
                  ? `Exponential is opening. You can close this tab and return to the app.`
                  : isPopup
                    ? `Returning you to Exponential — you can close this tab if it stays open.`
                    : `Your GitHub connections are updated. Continue to pick a repository.`}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {preview?.mobile ? (
                <Button asChild size="lg" className="w-full">
                  <a href={MOBILE_DEEP_LINK}>Return to the app</a>
                </Button>
              ) : (
                <Button asChild size="lg" className="w-full">
                  <Link to="/">Continue to Exponential</Link>
                </Button>
              )}
            </CardContent>
          </>
        ) : !preview ? (
          <CardHeader className="flex flex-col items-center gap-3 text-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <Github className="h-6 w-6" />
            </div>
            <CardTitle className="text-xl">Connecting GitHub…</CardTitle>
            <CardDescription>
              {ticket
                ? `Loading your GitHub accounts.`
                : `Missing claim ticket — restart the connect flow from team settings.`}
            </CardDescription>
          </CardHeader>
        ) : (
          <>
            <CardHeader className="flex flex-col items-center gap-3 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-muted text-foreground">
                <Github className="h-6 w-6" />
              </div>
              <CardTitle className="text-xl">Choose GitHub accounts</CardTitle>
              <CardDescription>
                Checked accounts are connected to this team. Uncheck a
                connected account to disconnect it.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                {preview.installations.map((inst) => {
                  const inUse =
                    inst.alreadyLinked && inst.activeRepoCount > 0
                  return (
                    <Label
                      key={inst.installationId}
                      className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-2.5 hover:bg-accent"
                    >
                      <Checkbox
                        checked={selected.has(inst.installationId)}
                        disabled={inUse || saving}
                        onCheckedChange={(value) => {
                          setSelected((prev) => {
                            const next = new Set(prev)
                            if (value === true) next.add(inst.installationId)
                            else next.delete(inst.installationId)
                            return next
                          })
                        }}
                      />
                      {inst.accountType === `Organization` ? (
                        <Building2 className="h-4 w-4 text-muted-foreground" />
                      ) : (
                        <User className="h-4 w-4 text-muted-foreground" />
                      )}
                      <span className="flex-1 truncate text-sm font-medium">
                        {inst.accountLogin ?? `Installation ${inst.installationId}`}
                      </span>
                      {inUse ? (
                        <span className="text-xs text-muted-foreground">
                          In use by {inst.activeRepoCount}{` `}
                          {inst.activeRepoCount === 1 ? `repo` : `repos`}
                        </span>
                      ) : null}
                    </Label>
                  )
                })}
              </div>
              {saveError ? (
                <p className="text-sm text-red-500">{saveError}</p>
              ) : null}
              {hasChanges || saving ? (
                <Button
                  size="lg"
                  className="w-full"
                  disabled={saving}
                  onClick={save}
                >
                  {saving ? `Saving…` : `Save`}
                </Button>
              ) : preview.mobile ? (
                <Button asChild size="lg" className="w-full">
                  <a href={MOBILE_DEEP_LINK}>Back to app</a>
                </Button>
              ) : isPopup ? (
                <Button size="lg" className="w-full" onClick={closePopup}>
                  Continue
                </Button>
              ) : (
                <Button asChild size="lg" className="w-full">
                  <Link to="/">Continue</Link>
                </Button>
              )}
            </CardContent>
          </>
        )}
      </Card>
    </div>
  )
}

export const Route = createFileRoute(`/_authenticated/integrations/github/claim`)(
  {
    validateSearch: (search: Record<string, unknown>): ClaimSearch => ({
      ticket: typeof search.ticket === `string` ? search.ticket : undefined,
      error: typeof search.error === `string` ? search.error : undefined,
      login: typeof search.login === `string` ? search.login : undefined,
      install: typeof search.install === `string` ? search.install : undefined,
    }),
    component: GithubClaim,
  }
)
