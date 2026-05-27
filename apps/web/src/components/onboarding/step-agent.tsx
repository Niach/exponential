import { useState } from "react"
import { Bot, Check, Copy, Terminal } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
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
import type { StepProps } from "./wizard"

export function StepAgent({ workspaceId, onNext, onSkip }: StepProps) {
  const [name, setName] = useState(`Companion`)
  const [installCommand, setInstallCommand] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [copied, setCopied] = useState(false)

  const handleCreate = async () => {
    if (!name.trim()) return
    setCreating(true)
    try {
      const result = await trpc.companion.create.mutate({
        workspaceId,
        name: name.trim(),
      })
      setInstallCommand(result.installCommand)
    } finally {
      setCreating(false)
    }
  }

  const handleCopy = async () => {
    if (!installCommand) return
    await navigator.clipboard.writeText(installCommand)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Bot className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Set up an AI agent</CardTitle>
        <CardDescription>
          Agents are local companions that can work on assigned issues
          autonomously. This is optional — you can always add agents later from
          workspace settings.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {installCommand ? (
          <div className="space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Terminal className="size-4" />
              Run this on your Linux server
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
                onClick={handleCopy}
                className="shrink-0"
              >
                {copied ? (
                  <Check className="size-4" />
                ) : (
                  <Copy className="size-4" />
                )}
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            <Label htmlFor="onb-agent-name">Agent name</Label>
            <div className="flex gap-2">
              <Input
                id="onb-agent-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Companion"
                autoFocus
              />
              <Button
                onClick={() => void handleCreate()}
                disabled={creating || !name.trim()}
              >
                <Bot className="mr-1.5 size-3.5" />
                {creating ? `Creating...` : `Create`}
              </Button>
            </div>
          </div>
        )}

        <div className="flex justify-between">
          <Button variant="ghost" onClick={onSkip}>
            Skip
          </Button>
          <Button onClick={onNext}>
            {installCommand ? `Finish setup` : `Skip`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
