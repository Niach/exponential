import { createFileRoute, Outlet, redirect } from "@tanstack/react-router"
import { authClient, authStateCollection } from "@/lib/auth/client"

function isSessionFresh(
  expiresAt: Date | string | undefined
): boolean {
  if (!expiresAt) return false
  const ts =
    expiresAt instanceof Date ? expiresAt.getTime() : Date.parse(String(expiresAt))
  return !Number.isNaN(ts) && ts > Date.now()
}

export const Route = createFileRoute(`/_authenticated`)({
  ssr: false,
  component: AuthenticatedLayout,
  beforeLoad: async () => {
    const cached = authStateCollection.get(`auth`)
    if (cached && isSessionFresh(cached.session?.expiresAt)) {
      return cached
    }

    const result = await authClient.getSession()

    if (!result.data?.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }

    const entry = {
      id: `auth` as const,
      session: result.data.session,
      user: result.data.user,
    }

    if (cached) {
      authStateCollection.update(`auth`, () => entry)
    } else {
      authStateCollection.insert(entry)
    }

    return result.data
  },
})

function AuthenticatedLayout() {
  return <Outlet />
}
