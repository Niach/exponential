import { useState } from "react"
import { Check, Copy, Link as LinkIcon, Loader2, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { useWorkspaceInvites } from "@/hooks/use-workspace-data"
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

export function WorkspaceInviteSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const [copied, setCopied] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [inviteUrl, setInviteUrl] = useState<string | null>(null)
  const invites = useWorkspaceInvites(workspaceId).filter(
    (invite) => !invite.acceptedAt
  )

  const handleGenerate = async () => {
    setGenerating(true)

    try {
      const { token } = await trpc.workspaceInvites.create.mutate({
        workspaceId,
      })

      setInviteUrl(`${window.location.origin}/invite/${token}`)
    } finally {
      setGenerating(false)
    }
  }

  const handleCopy = async () => {
    if (!inviteUrl) {
      return
    }

    await navigator.clipboard.writeText(inviteUrl)
    setCopied(true)

    setTimeout(() => setCopied(false), 2000)
  }

  const handleRevoke = async (id: string) => {
    await trpc.workspaceInvites.revoke.mutate({ id })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Invite Members</CardTitle>
        <CardDescription>
          Generate an invite link to share with people you want to add
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {inviteUrl && (
          <div className="flex items-center gap-2">
            <Input
              value={inviteUrl}
              readOnly
              className="text-xs font-mono"
              data-testid="invite-url-input"
            />
            <Button
              variant="outline"
              size="icon"
              onClick={handleCopy}
              className="shrink-0"
              aria-label="Copy invite URL"
            >
              {copied ? (
                <Check className="h-4 w-4" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
            </Button>
          </div>
        )}

        <Button onClick={handleGenerate} disabled={generating}>
          {generating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          <LinkIcon className="mr-2 h-4 w-4" />
          Generate invite link
        </Button>

        {invites.length > 0 && (
          <div className="space-y-2 pt-2">
            <div className="text-sm font-medium text-muted-foreground">
              Pending invites
            </div>
            {invites.map((invite) => (
              <div
                key={invite.id}
                className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
              >
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{invite.role}</Badge>
                  <span className="text-muted-foreground">
                    Expires{` `}
                    {new Date(invite.expiresAt).toLocaleDateString()}
                  </span>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => handleRevoke(invite.id)}
                  aria-label={`Revoke invite ${invite.id}`}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
