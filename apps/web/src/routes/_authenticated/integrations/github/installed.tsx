import { useEffect, useState } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { Check } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

// Landing page after a GitHub App install launched from the in-app project/repo
// dialog (state=dialog). Two very different arrivals share this page, and the
// mobile one is why EXP-18 exists:
//  • Desktop popup — opened by the repo picker via window.open(); the opener
//    re-detects the connection on focus, so we hand focus back and close the
//    popup after a beat.
//  • Mobile browser tab — window.open() with popup features opens a plain tab,
//    window.close() is a no-op there, and there's no opener to hand back to.
//    So on mobile the user is *left on this page*. It therefore has to stand on
//    its own: a real success state, comfortable mobile sizing, and a prominent
//    "Continue" action instead of a tiny confirmation that dead-ends the flow.
function GithubInstalled() {
  // Only a popup (has a distinct opener) can be auto-focused/closed. A mobile
  // or manually-opened tab has no opener, so we must not promise to "return"
  // them — we hand them a clear next step instead.
  const [isPopup] = useState(
    () =>
      typeof window !== `undefined` &&
      Boolean(window.opener) &&
      window.opener !== window
  )

  useEffect(() => {
    if (!isPopup) return
    const timer = setTimeout(() => {
      try {
        window.opener?.focus()
        window.close()
      } catch {
        // Blocked by the browser — the Continue button below is the fallback.
      }
    }, 600)
    return () => clearTimeout(timer)
  }, [isPopup])

  return (
    <div className="flex min-h-screen items-center justify-center p-4 sm:p-6">
      <Card className="w-full max-w-md">
        <CardHeader className="flex flex-col items-center gap-3 text-center">
          <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-500">
            <Check className="h-6 w-6" strokeWidth={2.5} />
          </div>
          <CardTitle className="text-xl">GitHub connected</CardTitle>
          <CardDescription>
            {isPopup
              ? `Returning you to Exponential — you can close this tab if it stays open.`
              : `Your repositories are now available. Continue to pick one for your project.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild size="lg" className="w-full">
            <Link to="/">Continue to Exponential</Link>
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}

export const Route = createFileRoute(
  `/_authenticated/integrations/github/installed`
)({
  component: GithubInstalled,
})
