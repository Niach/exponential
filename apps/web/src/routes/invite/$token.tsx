import { useCallback, useState, useEffect, useRef } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { trpc } from "@/lib/trpc-client"
import {
  Button,
  GlassCard,
  IconDisc,
} from "@exp/ui"
import { Users, LoaderCircle, CircleAlert, CircleCheck } from "lucide-react"
import { pageTitle } from "@/lib/page-title"
import {
  clearPendingInviteFor,
  readPendingInvite,
  rememberPendingInvite,
} from "@/lib/pending-invite"
import { JOIN_DEVICE_WAIT_MS, joinDeviceStep } from "@/lib/join-device-step"
import { useOwnDevices } from "@/components/device-setup"
import { DevicesStep } from "@/components/onboarding/devices-step"
import { WizardFrame } from "@/components/onboarding/wizard"

export const Route = createFileRoute(`/invite/$token`)({
  head: () => ({ meta: [{ title: pageTitle(`Team invite`) }] }),
  component: InviteAcceptPage,
  ssr: false,
})

function InviteAcceptPage() {
  const { token } = Route.useParams()
  const navigate = useNavigate()
  // EXP-1132: nothing renders until the session resolves — right after the
  // OAuth return a signed-in viewer used to see "Sign in or create account"
  // for a beat and be sent back to login.
  const { data: session, isPending: sessionPending } = useSession()
  const [accepting, setAccepting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // EXP-1169: the team this visit joined. Set on every successful accept;
  // `JoinedGate` then decides between the devices step and the team.
  const [joined, setJoined] = useState<JoinedTeam | null>(null)
  const success = joined !== null
  // EXP-630: a used invite may be the viewer's OWN placeholder invite — the
  // session hook stamps it accepted the moment they sign in through the
  // mailbox, before this page is reached. `getByToken` cannot tell (the
  // placeholder id is not public), so a signed-in viewer probes `accept`
  // once: it answers alreadyMember for the invitee and errors for anyone
  // else, who then sees the used-state as before. `null` = probing.
  const [usedForViewer, setUsedForViewer] = useState<boolean | null>(null)

  const [invite, setInvite] = useState<{
    teamName: string
    role: string
    acceptedAt: Date | null
    expiresAt: Date
  } | null>(null)
  const [loading, setLoading] = useState(true)

  // Fetch invite details on mount
  useEffect(() => {
    trpc.teamInvites.getByToken
      .query({ token })
      .then(({ invite }) => {
        setInvite({
          ...invite,
          acceptedAt: invite.acceptedAt ? new Date(invite.acceptedAt) : null,
          expiresAt: new Date(invite.expiresAt),
        })
        setLoading(false)
      })
      .catch((err) => {
        clearPendingInviteFor(token)
        setError(err.message || `Invalid or expired invite link`)
        setLoading(false)
      })
  }, [token])

  const handleAccept = async () => {
    setAccepting(true)
    setError(null)
    try {
      const { team } = await trpc.teamInvites.accept.mutate({
        token,
      })
      clearPendingInviteFor(token)
      setJoined({ id: team.id, slug: team.slug })
    } catch (err: unknown) {
      const message =
        err instanceof Error ? err.message : `Failed to accept invite`
      setError(message)
      setAccepting(false)
    }
  }

  const inviteUsed = !!invite?.acceptedAt
  const viewerLoggedIn = !!session?.user
  useEffect(() => {
    if (!inviteUsed || !viewerLoggedIn || usedForViewer !== null) return
    let cancelled = false
    trpc.teamInvites.accept
      .mutate({ token }, { context: { skipErrorToast: true } })
      .then(({ team }) => {
        if (cancelled) return
        clearPendingInviteFor(token)
        setUsedForViewer(false)
        setJoined({ id: team.id, slug: team.slug })
      })
      .catch(() => {
        if (cancelled) return
        clearPendingInviteFor(token)
        setUsedForViewer(true)
      })
    return () => {
      cancelled = true
    }
  }, [inviteUsed, viewerLoggedIn, usedForViewer, token])

  // EXP-1132: remember the invite across the sign-in detour; once the
  // visitor comes back signed in, finish what they started — the accept runs
  // by itself instead of asking for a second click. Anything that lands them
  // on onboarding instead resumes here from the remembered token.
  const inviteOpen =
    !!invite && !invite.acceptedAt && invite.expiresAt >= new Date()
  const autoAccepted = useRef(false)
  useEffect(() => {
    if (loading || sessionPending || !invite) return
    if (!inviteOpen) {
      if (!invite.acceptedAt) clearPendingInviteFor(token)
      return
    }
    if (!viewerLoggedIn) {
      rememberPendingInvite(token)
      return
    }
    if (autoAccepted.current || readPendingInvite() !== token) return
    autoAccepted.current = true
    // Consumed on the attempt: a failed accept (seat limit, ...) shows its
    // error here once and never bounces onboarding back to this page.
    clearPendingInviteFor(token)
    void handleAccept()
    // handleAccept is a fresh closure each render; the ref makes this once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loading, sessionPending, invite, inviteOpen, viewerLoggedIn, token])

  // The probe above decides whether a used invite is the viewer's own.
  const probing = inviteUsed && viewerLoggedIn && usedForViewer === null

  if (loading || sessionPending || (probing && !success)) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <LoaderCircle className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (joined) return <JoinedGate team={joined} />

  const isExpired = invite && invite.expiresAt < new Date()
  const isUsed = inviteUsed && (!viewerLoggedIn || usedForViewer === true)
  const isLoggedIn = viewerLoggedIn

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <GlassCard className="flex flex-col gap-6 py-6 backdrop-blur-md w-full max-w-md">
        <div className="grid gap-2 px-6 text-center">
          <IconDisc icon={Users} className="mx-auto mb-4" />
          <div className="leading-none font-semibold">
            {error && !invite ? `Invalid invite` : `Team invite`}
          </div>
          <div className="text-sm text-muted-foreground">
            {error && !invite
              ? error
              : invite
                ? `You've been invited to join`
                : ``}
          </div>
        </div>
        <div className="px-6 space-y-4">
          {invite && (
            <>
              <div className="rounded-lg border p-4 text-center">
                <div className="text-lg font-semibold">
                  {invite.teamName}
                </div>
                <div className="mt-1 text-sm text-muted-foreground">
                  Role: {invite.role}
                </div>
              </div>

              {isExpired ? (
                <div className="flex items-center gap-2 text-sm text-destructive">
                  <CircleAlert className="h-4 w-4" />
                  This invite has expired
                </div>
              ) : isUsed ? (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <CircleAlert className="h-4 w-4" />
                  This invite has already been used
                </div>
              ) : isLoggedIn ? (
                <>
                  {error && (
                    <div className="flex items-center gap-2 text-sm text-destructive">
                      <CircleAlert className="h-4 w-4" />
                      {error}
                    </div>
                  )}
                  <Button
                    className="w-full"
                    onClick={handleAccept}
                    disabled={accepting}
                  >
                    {accepting && (
                      <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
                    )}
                    Accept invite
                  </Button>
                </>
              ) : (
                /* Signup and login are one merged page (EXP-188), so a
                   single button covers both. */
                <Button className="w-full" asChild>
                  <Link
                    to="/auth/login"
                    search={{ redirect: `/invite/${token}` }}
                  >
                    Sign in or create account
                  </Link>
                </Button>
              )}
            </>
          )}

          {!invite && error && (
            <Button
              variant="outline"
              className="w-full"
              onClick={() =>
                navigate({
                  to: `/t/$teamSlug`,
                  params: { teamSlug: `default` },
                })
              }
            >
              Go to your team
            </Button>
          )}
        </div>
      </GlassCard>
    </div>
  )
}

type JoinedTeam = { id: string; slug: string }

// EXP-1169: after the accept. A joiner who owns no device gets the wizard's
// devices step (the creator's last step, skippable) before the team; one who
// has a machine goes straight in, as before. The decision is made ONCE: a
// device that registers while the step is open must not yank the page away,
// the step's own button turns into "Continue" instead.
function JoinedGate({ team }: { team: JoinedTeam }) {
  const navigate = useNavigate()
  const devices = useOwnDevices(team.id)
  const [timedOut, setTimedOut] = useState(false)
  const [decision, setDecision] = useState<`step` | `enter` | null>(null)

  useEffect(() => {
    const timer = window.setTimeout(() => setTimedOut(true), JOIN_DEVICE_WAIT_MS)
    return () => window.clearTimeout(timer)
  }, [])
  useEffect(() => {
    if (decision) return
    const next = joinDeviceStep(devices, timedOut)
    if (next !== `wait`) setDecision(next)
  }, [decision, devices, timedOut])

  const enter = useCallback(() => {
    void navigate({ to: `/t/$teamSlug`, params: { teamSlug: team.slug } })
  }, [navigate, team.slug])
  useEffect(() => {
    if (decision === `enter`) enter()
  }, [decision, enter])

  if (decision === `step`) {
    return (
      <WizardFrame>
        <DevicesStep teamId={team.id} onNext={enter} />
      </WizardFrame>
    )
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <GlassCard className="flex flex-col gap-6 py-6 backdrop-blur-md w-full max-w-md">
        <div className="grid gap-2 px-6 text-center">
          <IconDisc icon={Users} className="mx-auto mb-4" />
          <div className="leading-none font-semibold">Welcome!</div>
          <div className="text-sm text-muted-foreground">You&apos;ve joined the team.</div>
        </div>
        <div className="px-6">
          <div className="flex items-center justify-center gap-2 text-sm text-green-500">
            <CircleCheck className="h-4 w-4" />
            Successfully joined team
          </div>
        </div>
      </GlassCard>
    </div>
  )
}
