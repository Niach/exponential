import { useNavigate } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"

/** Sign out and return to the login page. Shared by the sidebar and mobile topbar menus. */
export function useSignOut() {
  const navigate = useNavigate()
  return async () => {
    await authClient.signOut()
    navigate({ to: `/auth/login`, search: { redirect: undefined } })
  }
}
