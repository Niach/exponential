import { useEffect, useState } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { AlertCircle, Building2, Check, Github, User } from "lucide-react"
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

// The OAuth claim flow's account picker: the callback verified (via GitHub's
// /user/installations) which App installations the user controls; when there
// are several, it lands here with a signed ticket and the user picks which
// GitHub accounts to connect to the workspace. Also the shared landing page
// for the claim flow's error states (?error=…). Mirrors installed.tsx's
// arrival modes: desktop popup (self-closes into the opener), mobile browser
// tab (exponential:// return card), plain tab (Continue link).
interface ClaimSearch {
  ticket?: string
  error?: string
}

const MOBILE_DEEP_LINK = githubConnectedDeepLink()

const ERROR_COPY: Record<string, { title: string; body: string }> = {
  session: {
    title: `Sign-in didn't match`,
    body: `This connect link expired or was started from a different session. Go back to workspace settings → Repositories and click Connect again.`,
  },
  exchange: {
    title: `GitHub authorization failed`,
    body: `GitHub didn't complete the authorization. Restart the connect flow from workspace settings → Repositories.`,
  },
  none: {
    title: `No installations found`,
    body: `That GitHub account has no Exponential App installations to connect. Use "Install on GitHub" from workspace settings → Repositories to install the App first.`,
  },
  forbidden: {
    title: `Not allowed`,
    body: `Only workspace owners can connect GitHub accounts to this workspace.`,
  },
}

interface PreviewInstallation {
  installationId: number
  accountLogin: string | null
  accountType: string | null
  alreadyLinked: boolean
}

function GithubClaim() {
  const { ticket, error } = Route.useSearch()
  const [preview, setPreview] = useState<{
    workspaceId: string
    mobile: boolean
    dialog: boolean
    installations: PreviewInstallation[]
  } | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [saving, setSaving] = useState(false)
  const [done, setDone] = useState(false)

  const [isPopup] = useState(
    () =>
      typeof window !== `undefined` &&
      Boolean(window.opener) &&
      window.opener !== window
  )

  useEffect(() => {
    if (!ticket) return
    let cancelled = false
    trpc.integrations.github.claimPreview
      .query({ ticket })
      .then((data) => {
        if (cancelled) return
        setPreview(data)
        // Preselect everything not yet linked — the common case is "connect
        // what I just authorized".
        setSelected(
          new Set(
            data.installations
              .filter((i) => !i.alreadyLinked)
              .map((i) => i.installationId)
          )
        )
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setLoadError(
          err instanceof Error ? err.message : `This claim link is invalid.`
        )
      })
    return () => {
      cancelled = true
    }
  }, [ticket])

  // Success: hand the user back the same way installed.tsx does.
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

  async function linkSelected() {
    if (!ticket || selected.size === 0) return
    setSaving(true)
    try {
      await trpc.integrations.github.claimLinks.mutate({
        ticket,
        installationIds: [...selected],
      })
      setDone(true)
    } catch (err) {
      setLoadError(
        err instanceof Error ? err.message : `Linking failed — try again.`
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4 sm:p-6">
      <Card className="w-full max-w-md">
        {errorCopy ? (
          <>
            <CardHeader className="flex flex-col items-center gap-3 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-red-500/15 text-red-500">
                <AlertCircle className="h-6 w-6" strokeWidth={2.5} />
              </div>
              <CardTitle className="text-xl">{errorCopy.title}</CardTitle>
              <CardDescription>{errorCopy.body}</CardDescription>
            </CardHeader>
            <CardContent>
              <Button asChild size="lg" className="w-full">
                <Link to="/">Back to Exponential</Link>
              </Button>
            </CardContent>
          </>
        ) : done ? (
          <>
            <CardHeader className="flex flex-col items-center gap-3 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-500">
                <Check className="h-6 w-6" strokeWidth={2.5} />
              </div>
              <CardTitle className="text-xl">GitHub connected</CardTitle>
              <CardDescription>
                {preview?.mobile
                  ? `Exponential is opening. You can close this tab and return to the app.`
                  : isPopup
                    ? `Returning you to Exponential — you can close this tab if it stays open.`
                    : `The selected GitHub accounts are now connected. Continue to pick a repository.`}
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
                : `Missing claim ticket — restart the connect flow from workspace settings.`}
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
                You have access to several installations of the Exponential
                App. Pick which to connect to this workspace.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                {preview.installations.map((inst) => {
                  const checked =
                    inst.alreadyLinked || selected.has(inst.installationId)
                  return (
                    <Label
                      key={inst.installationId}
                      className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-2.5 hover:bg-accent"
                    >
                      <Checkbox
                        checked={checked}
                        disabled={inst.alreadyLinked}
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
                      {inst.alreadyLinked ? (
                        <span className="text-xs text-muted-foreground">
                          Already connected
                        </span>
                      ) : null}
                    </Label>
                  )
                })}
              </div>
              <Button
                size="lg"
                className="w-full"
                disabled={saving || selected.size === 0}
                onClick={linkSelected}
              >
                {saving
                  ? `Connecting…`
                  : `Connect ${selected.size} ${selected.size === 1 ? `account` : `accounts`}`}
              </Button>
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
    }),
    component: GithubClaim,
  }
)
