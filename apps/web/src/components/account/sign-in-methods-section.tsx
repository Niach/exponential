import { useEffect, useRef, useState } from "react"
import {
  promptActions,
  removePasswordPrompt,
  unlinkSignInMethodPrompt,
} from "@/lib/prompts"
import { trpc } from "@/lib/trpc-client"
import { authClient } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { oauthLinkErrorMessage } from "@/lib/deep-link"
import {
  AppleIcon,
  GithubIcon,
  GoogleIcon,
} from "@/components/oauth-provider-buttons"
import { ChangeEmailDialog } from "@/components/account/change-email-dialog"
import {
  Button,
  GlassSectionHeader,
  ListRow,
  SETTINGS_LIST_CLASS,
  Prompt,
  conceptIcon,
  toast,
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

function showNotice(notice: Notice) {
  if (notice.tone === `error`) toast.error(notice.text)
  else toast.success(notice.text)
}

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
  if (provider.kind === `github`) return <GithubIcon />
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
  // The arrival outcome toasts ONCE (StrictMode re-runs effects, and
  // stripping the params below re-renders with an empty return).
  const arrivalToasted = useRef(false)
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
  // post-link state. Toast the outcome (the root mounts the Toaster BEFORE
  // the route tree, so it is already subscribed), then strip the params so
  // a reload does not repeat it.
  useEffect(() => {
    if (!linkReturn.linked && !linkReturn.linkError) return
    if (!arrivalToasted.current) {
      arrivalToasted.current = true
      const notice = noticeFromReturn(linkReturn, initialMethods.providers)
      if (notice) showNotice(notice)
    }
    onLinkReturnConsumed()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linkReturn.linked, linkReturn.linkError])

  const accountPath = `/t/${teamSlug}/settings/account`

  const link = async (provider: SignInProvider) => {
    if (pendingLink) return
    setPendingLink(provider.id)
    setError(``)
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
              provider: provider.id as `google` | `apple` | `github`,
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
      toast.success(
        unlinkTarget.kind === `password`
          ? `Password removed.`
          : `${unlinkTarget.name} unlinked.`
      )
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

  const unlinkCopy =
    unlinkTarget?.kind === `password`
      ? removePasswordPrompt()
      : unlinkSignInMethodPrompt(unlinkTarget?.name ?? `this method`)

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

      <ChangeEmailDialog
        open={changeOpen}
        currentEmail={methods.email}
        onOpenChange={setChangeOpen}
        onChanged={(email) => {
          setMethods((prev) => ({ ...prev, email, emailVerified: true }))
          toast.success(`Your email is now ${email}.`)
        }}
      />

      <Prompt
        open={unlinkTarget !== null}
        onOpenChange={(open) => {
          if (!open) setUnlinkTarget(null)
        }}
        busy={unlinking}
        title={unlinkCopy.title}
        body={unlinkCopy.body}
        actions={promptActions(unlinkCopy, {
          [unlinkTarget?.kind === `password` ? `remove` : `unlink`]: {
            busy: unlinking,
            onSelect: () => unlink(),
          },
        })}
      >
        {unlinkError ? (
          <p className="text-sm text-destructive">{unlinkError}</p>
        ) : null}
      </Prompt>
    </div>
  )
}
