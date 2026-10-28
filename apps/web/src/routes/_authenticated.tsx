import { useEffect } from "react"
import { createFileRoute, Outlet, redirect } from "@tanstack/react-router"
import { fetchSessionOnce } from "@/lib/auth/client"
import {
  getFirstTouch,
  hasFirstTouchParams,
} from "@/lib/conversion/first-touch"
import { trpc } from "@/lib/trpc-client"

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

let attributionClaimed = false
let timezoneClaimed = false

function AuthenticatedLayout() {
  // Cookieless signup attribution (EXP-362): ref/utm params ride URLs from
  // the marketing site through the auth flow; the first authenticated load
  // claims them for the account. Fire-and-forget and once per page load —
  // the server is the real gate (write-once, and only for accounts younger
  // than 24h), so stale params on an established account are a cheap no-op.
  useEffect(() => {
    if (attributionClaimed || !hasFirstTouchParams()) return
    attributionClaimed = true
    const touch = getFirstTouch()
    trpc.users.claimSignupAttribution
      .mutate({
        ref: touch.ref,
        utmSource: touch.utmSource,
        utmMedium: touch.utmMedium,
        utmCampaign: touch.utmCampaign,
        creemRef: touch.creemRef,
        referrer: touch.referrer,
        landingPath: touch.landingPath,
      })
      .catch(() => {
        // best-effort — never surface attribution failures to the user
      })
  }, [])

  // Timezone capture (EXP-369): the daily digest fires at a LOCAL hour, so the
  // server needs the account's zone. The browser is the only place that knows
  // it, so the first authenticated load claims it — `onlyIfUnset` keeps this a
  // one-time default that a later explicit pick in account settings (or
  // another client's claim) always wins over. Fire-and-forget, once per load.
  useEffect(() => {
    if (timezoneClaimed) return
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone
    if (!timezone) return
    timezoneClaimed = true
    trpc.users.setTimezone.mutate({ timezone, onlyIfUnset: true }).catch(() => {
      // best-effort — a missing timezone just means the digest uses UTC
    })
  }, [])

  return <Outlet />
}
