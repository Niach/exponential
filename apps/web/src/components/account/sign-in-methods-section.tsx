import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { authClient } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { oauthLinkErrorMessage } from "@/lib/deep-link"
import { AppleIcon, GoogleIcon } from "@/components/oauth-provider-buttons"
import { ChangeEmailDialog } from "@/components/account/change-email-dialog"
import {
  Button,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassSectionHeader,
  ListRow,
  SETTINGS_LIST_CLASS,
  conceptIcon,
} from "@exp/ui"
import type { SignInMethods, SignInProvider } from "@/lib/auth/sign-in-methods"

const MailIcon = conceptIcon(`ui-mail`)

// What the page arrived with after a provider round-trip: `?linked=<id>` on
// success, `?link_error=1&error=<reason>` on failure (Better Auth appends the
// reason to errorCallbackURL).
export interface LinkReturn {
  linked?: string
  linkError?: string
}

function formatDate(value: string | null): string {
  if (!value) return ``
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? `` : date.toLocaleDateString()
}

type Notice = { tone: `ok` | `error`; text: string }

function noticeFromReturn(
  linkReturn: LinkReturn,
  providers: SignInProvider[]
): Notice | null {
  if (linkReturn.linked) {
    const name =
      providers.find((p) => p.id === linkReturn.linked)?.name ?? linkReturn.linked
    return { tone: `ok`, text: `${name} linked to your account.` }
  }
  if (linkReturn.linkError) {
    return { tone: `error`, text: oauthLinkErrorMessage(linkReturn.linkError) }
  }
  return null
}

function ProviderMark({ provider }: { provider: SignInProvider }) {
  if (provider.kind === `google`) return <GoogleIcon />
  if (provider.kind === `apple`) return <AppleIcon />
  return null
}

// EXP-1126: Settings › Account › Sign-in methods. ONE list of every way into
// the account: the code to the primary email (always present, changeable),
// the configured providers (Apple, Google, OIDC) with Link/Unlink, and a
// password row while one is set. Passkeys keep their own band right below
// (PasskeysSection). Removals go through tRPC so the last-way-in rule is
// enforced in one place (lib/auth/sign-in-methods.ts).
export function SignInMethodsSection({
  initialMethods,
  teamSlug,
  linkReturn,
  onLinkReturnConsumed,
}: {
  initialMethods: SignInMethods
  teamSlug: string
  linkReturn: LinkReturn
  onLinkReturnConsumed: () => void
}) {
  const [methods, setMethods] = useState<SignInMethods>(initialMethods)
  const [error, setError] = useState(``)
  // Inline, not a toast: the settings pages carry their outcomes in place,
  // and a toast fired during the first render is lost before the Toaster
  // subscribes. Read once from the arrival URL, kept in state so stripping
  // the params (below) does not take it away.
  const [notice, setNotice] = useState<Notice | null>(() =>
    noticeFromReturn(linkReturn, initialMethods.providers)
  )
  const [pendingLink, setPendingLink] = useState<string | null>(null)
  const [unlinkTarget, setUnlinkTarget] = useState<SignInProvider | null>(null)
  const [unlinking, setUnlinking] = useState(false)
  const [unlinkError, setUnlinkError] = useState(``)
  const [changeOpen, setChangeOpen] = useState(false)

  const refresh = async () => {
    try {
      setMethods(await trpc.users.signInMethods.query())
    } catch (err) {
      console.error(`[sign-in-methods] refresh failed:`, err)
    }
  }

  // The provider round-trip lands back here: the loader already fetched the
  // post-link state; strip the params so a reload does not repeat the notice.
  useEffect(() => {
    if (!linkReturn.linked && !linkReturn.linkError) return
    onLinkReturnConsumed()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linkReturn.linked, linkReturn.linkError])

  const accountPath = `/t/${teamSlug}/settings/account`

  const link = async (provider: SignInProvider) => {
    if (pendingLink) return
    setPendingLink(provider.id)
    setError(``)
    setNotice(null)
    const callbackURL = `${accountPath}?linked=${encodeURIComponent(provider.id)}`
    const errorCallbackURL = `${accountPath}?link_error=1`
    try {
      const { error: startError } =
        provider.kind === `oidc`
          ? await authClient.oauth2.link({
              providerId: provider.id,
              callbackURL,
              errorCallbackURL,
            })
          : await authClient.linkSocial({
              provider: provider.id as `google` | `apple`,
              callbackURL,
              errorCallbackURL,
            })
      if (startError) {
        setError(authErrorMessage(startError, `Couldn't start linking. Try again.`))
        setPendingLink(null)
      }
      // On success the client follows Better Auth's redirect to the provider.
    } catch {
      setError(`Couldn't start linking. Try again.`)
      setPendingLink(null)
    }
  }

  const unlink = async () => {
    if (!unlinkTarget || unlinking) return
    setUnlinking(true)
    setUnlinkError(``)
    try {
      await trpc.users.unlinkSignInMethod.mutate({ providerId: unlinkTarget.id })
      setNotice({
        tone: `ok`,
        text:
          unlinkTarget.kind === `password`
            ? `Password removed.`
            : `${unlinkTarget.name} unlinked.`,
      })
      setUnlinkTarget(null)
      await refresh()
    } catch (err) {
      setUnlinkError(
        err instanceof Error ? err.message : `Couldn't remove that sign-in method.`
      )
    } finally {
      setUnlinking(false)
    }
  }

  const onlyWayIn = methods.waysIn <= 1

  return (
    <div>
      <GlassSectionHeader label="Sign-in methods" />
      <div className={SETTINGS_LIST_CLASS}>
        <ListRow className="justify-between gap-3 px-3 py-2">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex size-8 shrink-0 items-center justify-center text-muted-foreground [&_svg]:size-4">
              <MailIcon />
            </span>
            <div className="min-w-0">
              <div className="truncate text-sm font-medium">Email code</div>
              <div className="truncate text-xs text-muted-foreground">
                {methods.email}
                {methods.emailOtpEnabled
                  ? ``
                  : ` · sign-in codes are off on this instance (no mail transport)`}
              </div>
            </div>
          </div>
          {methods.emailOtpEnabled && (
            <Button
              size="sm"
              variant="outline"
              className="shrink-0"
              onClick={() => setChangeOpen(true)}
            >
              Change
            </Button>
          )}
        </ListRow>

        {methods.providers.map((provider) => {
          const blocked = provider.linked && onlyWayIn
          return (
            <ListRow key={provider.id} className="justify-between gap-3 px-3 py-2">
              <div className="flex min-w-0 items-center gap-3">
                <span className="flex size-8 shrink-0 items-center justify-center [&_svg]:size-4">
                  <ProviderMark provider={provider} />
                </span>
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{provider.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {provider.linked
                      ? blocked
                        ? `Linked · your only way to sign in`
                        : provider.available
                          ? `Linked${provider.linkedAt ? ` ${formatDate(provider.linkedAt)}` : ``}`
                          : `Linked · no longer offered on this instance`
                      : `Not linked`}
                  </div>
                </div>
              </div>
              {provider.linked ? (
                <Button
                  size="sm"
                  variant="outline"
                  className="shrink-0 text-destructive hover:text-destructive"
                  disabled={blocked}
                  onClick={() => {
                    setUnlinkError(``)
                    setUnlinkTarget(provider)
                  }}
                >
                  {provider.kind === `password` ? `Remove` : `Unlink`}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  className="shrink-0"
                  disabled={pendingLink !== null}
                  onClick={() => void link(provider)}
                >
                  {pendingLink === provider.id ? `Redirecting…` : `Link`}
                </Button>
              )}
            </ListRow>
          )
        })}
      </div>
      {error && <p className="mt-2 text-sm text-destructive">{error}</p>}
      {notice && (
        <p
          className={
            notice.tone === `error`
              ? `mt-2 text-sm text-destructive`
              : `mt-2 text-sm text-muted-foreground`
          }
          role="status"
        >
          {notice.text}
        </p>
      )}

      <ChangeEmailDialog
        open={changeOpen}
        currentEmail={methods.email}
        onOpenChange={setChangeOpen}
        onChanged={(email) => {
          setMethods((prev) => ({ ...prev, email, emailVerified: true }))
          setNotice({ tone: `ok`, text: `Your email is now ${email}.` })
        }}
      />

      <Dialog
        open={unlinkTarget !== null}
        onOpenChange={(open) => {
          if (!open) setUnlinkTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {unlinkTarget?.kind === `password`
                ? `Remove your password?`
                : `Unlink ${unlinkTarget?.name ?? `this method`}?`}
            </DialogTitle>
            <DialogDescription>
              {unlinkTarget?.kind === `password`
                ? `You will no longer be able to sign in with a password. Your other sign-in methods keep working.`
                : `${unlinkTarget?.name ?? `It`} will no longer sign you in. You can link it again any time; your other sign-in methods keep working.`}
            </DialogDescription>
          </DialogHeader>
          {unlinkError && (
            <p className="px-6 text-sm text-destructive">{unlinkError}</p>
          )}
          <DialogFooter>
            <DialogCancel variant="outline" onClick={() => setUnlinkTarget(null)} />
            <Button variant="destructive" onClick={() => void unlink()} disabled={unlinking}>
              {unlinking
                ? `Removing…`
                : unlinkTarget?.kind === `password`
                  ? `Remove`
                  : `Unlink`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
