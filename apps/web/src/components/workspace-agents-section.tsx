import { useCallback, useEffect, useState } from "react"
import {
  Bot,
  Check,
  Copy,
  Loader2,
  RotateCcw,
  Terminal,
  Trash2,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

type CompanionAgentList = Awaited<
  ReturnType<typeof trpc.companion.list.query>
>[`agents`]

function formatSeen(value: Date | string | null): string {
  if (!value) return `Never seen`
  const date = value instanceof Date ? value : new Date(value)
  return `Last seen ${date.toLocaleString()}`
}

export function WorkspaceAgentsSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const [agents, setAgents] = useState<CompanionAgentList>([])
  const [loading, setLoading] = useState(true)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState(`Companion`)
  const [installCommand, setInstallCommand] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const refresh = useCallback(async () => {
    const result = await trpc.companion.list.query({ workspaceId })
    setAgents(result.agents)
    setLoading(false)
  }, [workspaceId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  const copyInstallCommand = async () => {
    if (!installCommand) return
    await navigator.clipboard.writeText(installCommand)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }

  const createAgent = async () => {
    setCreating(true)
    try {
      const result = await trpc.companion.create.mutate({
        workspaceId,
        name,
      })
      setInstallCommand(result.installCommand)
      await refresh()
    } finally {
      setCreating(false)
    }
  }

  const regenerateSetup = async (agentId: string) => {
    setBusyId(agentId)
    try {
      const result = await trpc.companion.regenerateSetup.mutate({ agentId })
      setInstallCommand(result.installCommand)
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  const revokeAgent = async (agentId: string) => {
    if (!window.confirm(`Remove this agent member and revoke its API key?`)) {
      return
    }
    setBusyId(agentId)
    try {
      await trpc.companion.revoke.mutate({ agentId })
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Bot className="h-4 w-4" />
          Agent Members
        </CardTitle>
        <CardDescription>
          Install local companions that can work assigned issues. Notifications
          arrive on your mobile apps via push. To remove a companion later, run
          on the host:
          {` `}
          <code className="rounded bg-muted px-1 py-0.5 text-xs">
            curl -fsSL {window.location.origin}/install/uninstall.sh | bash
          </code>
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {installCommand && (
          <div className="rounded-md border bg-muted/40 p-3">
            <div className="mb-2 flex items-center gap-2 text-sm font-medium">
              <Terminal className="h-4 w-4" />
              Linux install command
            </div>
            <div className="flex min-w-0 items-center gap-2">
              <Input
                value={installCommand}
                readOnly
                className="min-w-0 flex-1 font-mono text-xs"
              />
              <Button
                variant="outline"
                size="icon"
                className="shrink-0"
                onClick={copyInstallCommand}
                aria-label="Copy install command"
              >
                {copied ? (
                  <Check className="h-4 w-4" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>
        )}

        <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
          <div className="grid flex-1 gap-2">
            <Label htmlFor="agent-name">Agent name</Label>
            <Input
              id="agent-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <Button onClick={createAgent} disabled={creating || !name.trim()}>
            {creating && <Loader2 className="h-4 w-4 animate-spin" />}
            <Bot className="h-4 w-4" />
            Add agent member
          </Button>
        </div>

        <div className="space-y-2">
          {loading ? (
            <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              Loading agents
            </div>
          ) : agents.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No agent members yet.
            </div>
          ) : (
            agents.map((agent) => (
              <div
                key={agent.id}
                className="flex flex-col gap-3 overflow-hidden rounded-md border px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="break-all text-sm font-medium">
                      {agent.name}
                    </span>
                    <Badge variant="secondary">agent</Badge>
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground">
                    {agent.email} · {formatSeen(agent.lastSeenAt)}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => regenerateSetup(agent.id)}
                    disabled={busyId === agent.id}
                    aria-label={`Regenerate setup for ${agent.name}`}
                  >
                    <RotateCcw className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => revokeAgent(agent.id)}
                    disabled={busyId === agent.id}
                    aria-label={`Remove ${agent.name}`}
                  >
                    <Trash2 className="h-4 w-4 text-destructive" />
                  </Button>
                </div>
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  )
}
