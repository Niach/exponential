import { useEffect } from "react"
import { useRouterState } from "@tanstack/react-router"
import { toast } from "@exp/ui"
import { LINK_NO_ACCESS } from "@/lib/app-link"

// #EXP-1674: a link to a team the viewer is not in redirects home from a route
// guard, before anything can toast. The guard leaves a note; the root shows it
// once the redirect has landed. `sessionStorage` is browser-only, so a guard
// running on the server leaves none.
const KEY = `exp-link-no-access`

export function queueLinkToast(): void {
  if (typeof window !== `undefined`) window.sessionStorage.setItem(KEY, `1`)
}

export function LinkToast() {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  useEffect(() => {
    if (window.sessionStorage.getItem(KEY) === null || pathname === `/`) return
    window.sessionStorage.removeItem(KEY)
    toast.error(LINK_NO_ACCESS)
  }, [pathname])
  return null
}
