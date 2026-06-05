import { useEffect, useState } from "react"
import { createFileRoute, Link } from "@tanstack/react-router"
import { Github } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

// Landing page after a GitHub App install that was launched from the in-app
// project/agent dialog (state=dialog). The dialog opened this in a separate
// tab/popup and re-detects the connection when it regains focus, so all this
// page does is confirm and try to hand focus back. We avoid postMessage/popup
// relays (brittle with blockers); the opener simply re-queries on focus.
function GithubInstalled() {
  const [closed, setClosed] = useState(false)

  useEffect(() => {
    const timer = setTimeout(() => {
      try {
        window.opener?.focus()
        window.close()
      } catch {
        // Manually-opened tab (no opener) — fall through to the message below.
      }
      setClosed(true)
    }, 600)
    return () => clearTimeout(timer)
  }, [])

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Github className="h-4 w-4" />
            GitHub connected
          </CardTitle>
          <CardDescription>
            {closed
              ? `You can close this tab and return to Exponential — your repos are now available.`
              : `Returning you to Exponential…`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild variant="outline">
            <Link to="/">Back to Exponential</Link>
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
