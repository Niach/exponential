import { useNavigate } from "@tanstack/react-router"
import { blockingLabel, whileBlocking } from "@exp/ui"
import { authClient, invalidateSessionCache } from "@/lib/auth/client"
import { disposeAllSteerSessions } from "@/lib/steer-session-store"
import { signOutWebInstall } from "@/lib/enet/use-web-install"

/** Sign out and return to the login page. Shared by the sidebar and mobile topbar menus.
 *  EXP-1275: behind the blocking overlay ("Signing out") until the login page
 *  is up. */
export function useSignOut() {
  const navigate = useNavigate()
  return () =>
    whileBlocking(blockingLabel(`signOut`), async () => {
      // Background steer connections (EXP-621) are account-scoped state —
      // close them before the session dies so nothing keeps streaming.
      disposeAllSteerSessions()
      // EXP-1457: an explicit sign-out retires this install's eNet identity
      // (revoked server-side, forgotten locally). Best-effort: a failure
      // never blocks signing out.
      await signOutWebInstall().catch(() => {})
      await authClient.signOut()
      // SPA navigation keeps the module alive — without this,
      // fetchSessionOnce's 30s cache would let the signed-out user back
      // through the auth guard.
      invalidateSessionCache()
      await navigate({ to: `/auth/login`, search: { redirect: undefined } })
    })
}
