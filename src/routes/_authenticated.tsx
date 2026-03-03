import { createFileRoute, Outlet, redirect } from "@tanstack/react-router"
import { authClient, authStateCollection } from "@/lib/auth-client"

export const Route = createFileRoute(`/_authenticated`)({
  ssr: false,
  component: AuthenticatedLayout,
  beforeLoad: async () => {
    const cached = authStateCollection.get(`auth`)
    if (cached && cached.session?.expiresAt > new Date()) {
      return cached
    }

    const result = await authClient.getSession()

    if (!result.data?.session) {
      throw redirect({ to: `/auth/login` })
    }

    authStateCollection.insert({
      id: `auth`,
      session: result.data.session,
      user: result.data.user,
    })

    return result.data
  },
})

function AuthenticatedLayout() {
  return <Outlet />
}
