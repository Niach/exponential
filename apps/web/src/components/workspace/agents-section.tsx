import { useCallback, useEffect, useState } from "react"
import { Bot, Loader2, Monitor, Trash2 } from "lucide-react"
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

type CompanionAgentList = Awaited<
  ReturnType<typeof trpc.agent.list.query>
>[`agents`]

type MineAgentList = Awaited<
  ReturnType<typeof trpc.agent.listMine.query>
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
  const [elsewhere, setElsewhere] = useState<MineAgentList>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [result, mine] = await Promise.all([
        trpc.agent.list.query({ workspaceId }),
        trpc.agent.listMine.query(),
      ])
      setAgents(result.agents)
      // Agents this account registered against a DIFFERENT workspace — the
      // pre-fix "wrong workspace" orphan case. Surface so it can be revoked.
      setElsewhere(mine.agents.filter((a) => a.workspaceId !== workspaceId))
      setError(null)
    } catch {
      // A FORBIDDEN/NOT_FOUND (e.g. non-owner) used to reject silently and
      // leave the panel stuck on the loading spinner. Surface it instead.
      setError(
        `Couldn't load agents — are you the owner of this workspace?`
      )
    } finally {
      setLoading(false)
    }
  }, [workspaceId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  const revokeAgent = async (agentId: string) => {
    if (
      !window.confirm(
        `Remove this agent? Its credential is revoked and it stops working until re-registered.`
      )
    ) {
      return
    }
    setBusyId(agentId)
    try {
      await trpc.agent.revoke.mutate({ agentId })
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
          Agents
        </CardTitle>
        <CardDescription>
          Agents run coding sessions on assigned issues. Register a machine as an
          agent from the Exponential desktop app — open Settings → Agents and
          click “Register this machine”. It authenticates with your account; no
          tokens to copy.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
          <Monitor className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            Registration happens in the desktop app (Linux & macOS). Registered
            machines appear here, where you can revoke them.
          </span>
        </div>

        <div className="space-y-2">
          {loading ? (
            <div className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              Loading agents
            </div>
          ) : error ? (
            <div className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          ) : agents.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No agents registered yet.
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
                    {agent.ownerName ? `Owned by ${agent.ownerName} · ` : ``}
                    {formatSeen(agent.lastSeenAt)}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
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

        {!loading && elsewhere.length > 0 && (
          <div className="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-3">
            <p className="text-xs text-muted-foreground">
              This account has an agent registered in another workspace
              {elsewhere.some((a) => a.workspaceIsPublic)
                ? ` (likely registered before a fix that pinned agents to your own workspace)`
                : ``}
              . Revoke it if it’s an orphan, then re-register from the desktop
              app.
            </p>
            {elsewhere.map((agent) => (
              <div
                key={agent.id}
                className="flex items-center justify-between gap-2 rounded-md border px-3 py-2"
              >
                <div className="min-w-0">
                  <span className="break-all text-sm font-medium">
                    {agent.name}
                  </span>
                  <div className="truncate text-xs text-muted-foreground">
                    in {agent.workspaceName} · {formatSeen(agent.lastSeenAt)}
                  </div>
                </div>
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
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
