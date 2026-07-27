import { createFileRoute, Outlet, redirect } from "@tanstack/react-router"
import { fetchSessionOnce } from "@/lib/auth/client"

export const Route = createFileRoute(`/_authenticated`)({
  ssr: false,
  component: AuthenticatedLayout,
  beforeLoad: async ({ location }) => {
    const sessionData = await fetchSessionOnce()

    if (!sessionData) {
      // Carry the destination through the login hop so emailed/copied deep
      // links survive it. `location.href` is origin-stripped; the login page
      // re-clamps it with sanitizeRedirectPath, so this adds no open-redirect
      // surface.
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }

    return {
      session: sessionData.session,
      user: sessionData.user,
    }
  },
})

function AuthenticatedLayout() {
  return <Outlet />
}
