import { useNavigate } from "@tanstack/react-router"
import { authClient, invalidateSessionCache } from "@/lib/auth/client"

/** Sign out and return to the login page. Shared by the sidebar and mobile topbar menus. */
export function useSignOut() {
  const navigate = useNavigate()
  return async () => {
    await authClient.signOut()
    // SPA navigation keeps the module alive — without this, fetchSessionOnce's
    // 30s cache would let the signed-out user back through the auth guard.
    invalidateSessionCache()
    navigate({ to: `/auth/login`, search: { redirect: undefined } })
  }
}
