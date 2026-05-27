import { createFileRoute, Outlet, redirect } from "@tanstack/react-router"
import { fetchSessionOnce } from "@/lib/auth/client"

export const Route = createFileRoute(`/_authenticated`)({
  ssr: false,
  component: AuthenticatedLayout,
  beforeLoad: async () => {
    const sessionData = await fetchSessionOnce()

    if (!sessionData) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
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
