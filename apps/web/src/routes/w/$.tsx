import { createFileRoute, redirect } from "@tanstack/react-router"

// Legacy /w/ → /t/ (workspaces → teams rename). Production serves a real 301
// from server-bun.ts before the router ever runs; this client-side splat
// covers dev (the nitro-alpha bridge bypasses server-bun.ts) and any
// SPA-internal navigation that still carries an old /w/ href. `/w/` acceptance
// is permanent — old links live in the wild forever.
export const Route = createFileRoute(`/w/$`)({
  ssr: false,
  beforeLoad: ({ params, location }) => {
    throw redirect({
      href: `/t/${params._splat ?? ``}${location.searchStr}`,
      replace: true,
    })
  },
})
