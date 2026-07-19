import { createFileRoute, redirect } from "@tanstack/react-router"
import { sanitizeRedirectPath } from "@/lib/auth/safe-redirect"

// EXP-188: signup and login are ONE merged /auth/login page (with a
// create-account toggle when sign-up is open). This route survives only as
// an unconditional redirect so old links and bookmarks keep working. The
// full search is forwarded — an in-flight OAuth authorize query (client_id,
// redirect_uri, ...) must survive the hop to login.
export const Route = createFileRoute(`/auth/register`)({
  ssr: false,
  loader: ({ location }) => {
    throw redirect({
      to: `/auth/login`,
      search: location.search as Record<string, unknown>,
    })
  },
  // Pass unknown params through — an in-flight OAuth authorize query
  // (client_id, redirect_uri, ...) must survive router normalization.
  validateSearch: (
    search: Record<string, unknown>
  ): { redirect?: string } & Record<string, unknown> => ({
    ...search,
    redirect: sanitizeRedirectPath(search.redirect),
  }),
})
