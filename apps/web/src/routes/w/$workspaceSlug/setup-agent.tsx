import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  ArrowLeft,
  Bot,
  Check,
  Circle,
  Github,
  Loader2,
  Monitor,
} from "lucide-react"
import { useWorkspaceBySlug } from "@/hooks/use-workspace-data"
import { useSetupChecklist } from "@/hooks/use-setup-checklist"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export const Route = createFileRoute(`/w/$workspaceSlug/setup-agent`)({
  beforeLoad: ({ context }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: undefined } })
    }
  },
  component: SetupAgent,
})

function StepCard({
  done,
  index,
  icon: Icon,
  title,
  description,
  children,
}: {
  done: boolean
  index: number
  icon: typeof Bot
  title: string
  description: string
  children?: React.ReactNode
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          {done ? (
            <Check className="size-4 text-green-500" />
          ) : (
            <Circle className="size-4 text-muted-foreground" />
          )}
          <Icon className="size-4" />
          <span>
            {index}. {title}
          </span>
        </CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      {children && <CardContent>{children}</CardContent>}
    </Card>
  )
}

function SetupAgent() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { status } = useSetupChecklist(workspace?.id)
  const [github, setGithub] = useState<{
    installed: boolean
    installUrl: string | null
    accounts: string[]
  } | null>(null)

  useEffect(() => {
    void trpc.integrations.github.status.query().then((s) =>
      setGithub({
        installed: s.installed,
        installUrl: s.installUrl,
        accounts: s.accounts,
      })
    )
    const onFocus = () =>
      void trpc.integrations.github.status.query().then((s) =>
        setGithub({
          installed: s.installed,
          installUrl: s.installUrl,
          accounts: s.accounts,
        })
      )
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [])

  return (
    <div className="mx-auto max-w-2xl space-y-6 p-6">
      <div>
        <Button
          variant="ghost"
          size="sm"
          className="mb-2 -ml-2 text-muted-foreground"
          onClick={() =>
            navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug } })
          }
        >
          <ArrowLeft className="mr-1 size-4" />
          Back
        </Button>
        <h1 className="flex items-center gap-2 text-2xl font-bold">
          <Bot className="size-6" />
          Set up a coding agent
        </h1>
        <p className="text-sm text-muted-foreground">
          Connect a repo and a desktop machine, then assign issues to your agent
          to have it plan, get your approval, and open pull requests.
        </p>
      </div>

      <StepCard
        index={1}
        done={Boolean(status?.hasProject)}
        icon={Bot}
        title="Create a project"
        description="A project is a connected GitHub repo where the agent works."
      >
        <Button
          variant="outline"
          onClick={() =>
            navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug } })
          }
        >
          Go to projects
        </Button>
      </StepCard>

      <StepCard
        index={2}
        done={Boolean(github?.installed)}
        icon={Github}
        title="Connect GitHub"
        description="Install the Exponential GitHub App so the agent can push branches and open PRs."
      >
        {github?.installed ? (
          <p className="text-sm text-muted-foreground">
            Connected{github.accounts.length ? ` · ${github.accounts.join(`, `)}` : ``}
          </p>
        ) : github?.installUrl ? (
          <Button
            onClick={() =>
              window.open(github.installUrl!, `gh-install`, `popup,width=980,height=820`)
            }
          >
            <Github className="mr-2 size-4" />
            Connect GitHub
          </Button>
        ) : (
          <p className="text-sm text-muted-foreground">
            GitHub isn’t configured on this server.
          </p>
        )}
      </StepCard>

      <StepCard
        index={3}
        done={Boolean(status?.machineRegistered)}
        icon={Monitor}
        title="Install & register the desktop app"
        description="The agent runs on your machine. Open the Exponential desktop app, sign in, and register this machine as an agent."
      >
        {status?.machineRegistered ? (
          <p className="text-sm text-muted-foreground">
            {status.agentSeen
              ? `A desktop agent is registered and has checked in.`
              : `Registered — waiting for the agent to come online.`}
          </p>
        ) : (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            Waiting for a machine to register…
          </p>
        )}
      </StepCard>

      <StepCard
        index={4}
        done={Boolean(status?.firstIssueAssignedToAgent)}
        icon={Bot}
        title="Assign an issue to the agent"
        description="Open any issue and set the Assignee to your agent — it'll start planning."
      >
        <Button
          variant="outline"
          onClick={() =>
            navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug } })
          }
        >
          Go to projects
        </Button>
      </StepCard>
    </div>
  )
}
