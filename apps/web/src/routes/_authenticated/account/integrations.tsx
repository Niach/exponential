import { createFileRoute, Link } from "@tanstack/react-router"
import { ArrowLeft, ExternalLink, Github } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getAuthConfig } from "@/lib/auth/config"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export const Route = createFileRoute(`/_authenticated/account/integrations`)({
  loader: async () => {
    const [authConfig, githubStatus] = await Promise.all([
      getAuthConfig(),
      trpc.integrations.github.status.query(),
    ])
    return { authConfig, githubStatus }
  },
  component: AccountIntegrations,
})

function AccountIntegrations() {
  const { authConfig, githubStatus } = Route.useLoaderData()

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <Button asChild variant="ghost" size="sm" className="-ml-2 mb-2">
          <Link to="/">
            <ArrowLeft className="h-4 w-4" />
            Back
          </Link>
        </Button>
        <h1 className="text-2xl font-bold">Integrations</h1>
        <p className="text-sm text-muted-foreground">
          Connect external services to your account.
        </p>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-md border bg-muted">
              <Github className="h-5 w-5" />
            </div>
            <div className="flex-1">
              <CardTitle>GitHub</CardTitle>
              <CardDescription>
                Install the Exponential GitHub App on the repos you want to code
                on. It opens pull requests, reads diffs, and lets your desktop
                coding sessions clone + push — scoped to just those repos.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {!authConfig.githubEnabled ? (
            <div className="text-sm text-muted-foreground">
              GitHub is not configured on this server. Set
              <code className="mx-1 rounded bg-muted px-1 py-0.5 text-xs">
                GITHUB_APP_ID
              </code>
              and
              <code className="mx-1 rounded bg-muted px-1 py-0.5 text-xs">
                GITHUB_APP_PRIVATE_KEY
              </code>
              to enable it.
            </div>
          ) : githubStatus.installed ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2 text-sm">
                <div className="h-2 w-2 rounded-full bg-green-500" />
                <span>
                  Installed
                  {githubStatus.accounts.length > 0
                    ? ` · ${githubStatus.accounts.join(`, `)}`
                    : ``}
                </span>
              </div>
              {githubStatus.installUrl && (
                <Button asChild variant="outline">
                  <a href={githubStatus.installUrl}>
                    <ExternalLink className="h-4 w-4" />
                    Manage / add repos
                  </a>
                </Button>
              )}
            </div>
          ) : (
            githubStatus.installUrl && (
              <Button asChild>
                <a href={githubStatus.installUrl}>
                  <ExternalLink className="h-4 w-4" />
                  Install GitHub App
                </a>
              </Button>
            )
          )}
        </CardContent>
      </Card>
    </div>
  )
}
