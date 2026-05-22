import { useCallback, useEffect, useMemo, useState } from "react"
import QRCode from "qrcode"
import {
  Bot,
  Check,
  Copy,
  Loader2,
  QrCode,
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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

function statusVariant(status: string): `default` | `secondary` | `outline` {
  if (status === `connected`) return `default`
  if (status === `qr` || status === `pairing_requested`) return `secondary`
  return `outline`
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
  const [pairingAgentId, setPairingAgentId] = useState<string | null>(null)
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    const result = await trpc.companion.list.query({ workspaceId })
    setAgents(result.agents)
    setLoading(false)
  }, [workspaceId])

  useEffect(() => {
    setLoading(true)
    void refresh()
  }, [refresh])

  useEffect(() => {
    if (!pairingAgentId) return
    const timer = window.setInterval(() => void refresh(), 2500)
    return () => window.clearInterval(timer)
  }, [pairingAgentId, refresh])

  const pairingAgent = useMemo(
    () => agents.find((agent) => agent.id === pairingAgentId) ?? null,
    [agents, pairingAgentId]
  )

  useEffect(() => {
    if (!pairingAgent?.whatsappQr) {
      setQrDataUrl(null)
      return
    }

    let cancelled = false
    void QRCode.toDataURL(pairingAgent.whatsappQr, {
      margin: 1,
      width: 256,
      color: {
        dark: `#09090b`,
        light: `#ffffff`,
      },
    }).then((url) => {
      if (!cancelled) setQrDataUrl(url)
    })

    return () => {
      cancelled = true
    }
  }, [pairingAgent?.whatsappQr])

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
      if (pairingAgentId === agentId) setPairingAgentId(null)
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  const requestWhatsappPairing = async (agentId: string) => {
    setBusyId(agentId)
    setPairingAgentId(agentId)
    try {
      await trpc.companion.requestWhatsappPairing.mutate({ agentId })
      await refresh()
    } finally {
      setBusyId(null)
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Bot className="h-4 w-4" />
            Agent Members
          </CardTitle>
          <CardDescription>
            Install local companions that can work assigned issues and send
            WhatsApp updates.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {installCommand && (
            <div className="rounded-md border bg-muted/40 p-3">
              <div className="mb-2 flex items-center gap-2 text-sm font-medium">
                <Terminal className="h-4 w-4" />
                Linux install command
              </div>
              <div className="flex items-center gap-2">
                <Input
                  value={installCommand}
                  readOnly
                  className="font-mono text-xs"
                />
                <Button
                  variant="outline"
                  size="icon"
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
                  className="flex flex-col gap-3 rounded-md border px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium">{agent.name}</span>
                      <Badge variant="secondary">agent</Badge>
                      <Badge variant={statusVariant(agent.whatsappStatus)}>
                        WhatsApp {agent.whatsappStatus.replace(/_/g, ` `)}
                      </Badge>
                    </div>
                    <div className="mt-1 truncate text-xs text-muted-foreground">
                      {agent.email} · {formatSeen(agent.lastSeenAt)}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => requestWhatsappPairing(agent.id)}
                      disabled={busyId === agent.id}
                    >
                      {busyId === agent.id ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <QrCode className="h-4 w-4" />
                      )}
                      WhatsApp
                    </Button>
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

      <Dialog
        open={Boolean(pairingAgentId)}
        onOpenChange={(open) => {
          if (!open) setPairingAgentId(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Connect WhatsApp</DialogTitle>
            <DialogDescription>
              Scan this code from WhatsApp Linked Devices for{" "}
              {pairingAgent?.name}.
            </DialogDescription>
          </DialogHeader>
          <div className="flex min-h-[18rem] items-center justify-center rounded-md border bg-white p-4">
            {qrDataUrl ? (
              <img
                src={qrDataUrl}
                alt="WhatsApp pairing QR"
                className="h-64 w-64"
              />
            ) : pairingAgent?.whatsappStatus === `connected` ? (
              <div className="flex items-center gap-2 text-sm text-zinc-950">
                <Check className="h-4 w-4" />
                WhatsApp connected
              </div>
            ) : (
              <div className="flex items-center gap-2 text-sm text-zinc-950">
                <Loader2 className="h-4 w-4 animate-spin" />
                Waiting for companion
              </div>
            )}
          </div>
          {pairingAgent?.whatsappLastError && (
            <div className="rounded-md border border-destructive/40 px-3 py-2 text-sm text-destructive">
              {pairingAgent.whatsappLastError}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}
