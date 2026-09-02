import { useState } from "react"
import { Check, Copy } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

type ApiKeyRow = Awaited<
  ReturnType<typeof trpc.users.listPersonalApiKeys.query>
>[`keys`][number]

// Desktop/CLI sign-ins auto-mint their hidden key under this name prefix
// (crates/api device_key_name) — revoking one of those signs that device's
// agent/MCP wiring out until it re-mints on the next session.
const DEVICE_KEY_PREFIX = `Device: `

function formatDate(value: Date | string | null): string {
  if (!value) return `–`
  const date = value instanceof Date ? value : new Date(value)
  return Number.isNaN(date.getTime()) ? `–` : date.toLocaleDateString()
}

function keyPreview(row: ApiKeyRow): string {
  // Better Auth stores the visible prefix chars in `start`; `prefix` is the
  // static expu_ marker. Show whatever is available without leaking length.
  if (row.start) return `${row.start}…`
  if (row.prefix) return `${row.prefix}…`
  return `expu_…`
}

// Self-service personal API keys (EXP-238). Keys act as the signed-in user
// everywhere a session does: the MCP endpoint, tRPC, sync — and the CLI's
// EXP_TOKEN. The raw key exists client-side only in the mint dialog.
export function ApiKeysSection({ initialKeys }: { initialKeys: ApiKeyRow[] }) {
  const [keys, setKeys] = useState<ApiKeyRow[]>(initialKeys)
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState(``)
  const [minting, setMinting] = useState(false)
  const [mintError, setMintError] = useState(``)
  const [mintedKey, setMintedKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyRow | null>(null)
  const [revoking, setRevoking] = useState(false)

  const closeCreate = () => {
    setCreateOpen(false)
    setName(``)
    setMintError(``)
    setMintedKey(null)
    setCopied(false)
  }

  const handleMint = async () => {
    if (minting) return
    setMinting(true)
    setMintError(``)
    try {
      const created = await trpc.users.mintPersonalApiKey.mutate(
        name.trim() ? { name: name.trim() } : undefined
      )
      // The mutation returns the display metadata too, so the list updates
      // without a follow-up query.
      setKeys((prev) => [
        {
          id: created.id,
          name: created.name,
          start: created.start,
          prefix: created.prefix,
          createdAt: created.createdAt,
          lastRequest: null,
        },
        ...prev,
      ])
      setMintedKey(created.key)
    } catch (err) {
      setMintError(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't create the key`
      )
    } finally {
      setMinting(false)
    }
  }

  const handleCopy = async () => {
    if (!mintedKey) return
    try {
      await navigator.clipboard.writeText(mintedKey)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard denied — the key is selectable in the code block.
    }
  }

  const handleRevoke = async () => {
    if (!revokeTarget || revoking) return
    setRevoking(true)
    try {
      await trpc.users.revokePersonalApiKey.mutate({ id: revokeTarget.id })
      setKeys((prev) => prev.filter((row) => row.id !== revokeTarget.id))
      setRevokeTarget(null)
    } catch (err) {
      console.error(`[api-keys] revoke failed:`, err)
    } finally {
      setRevoking(false)
    }
  }

  const revokeIsDeviceKey = Boolean(
    revokeTarget?.name?.startsWith(DEVICE_KEY_PREFIX)
  )

  return (
    <div className="space-y-6">
      <div>
        <GlassSectionHeader
          label="API keys"
          trailing={
            <Button
              variant="glass"
              size="xs"
              onClick={() => setCreateOpen(true)}
            >
              Create key
            </Button>
          }
        />
        <p className="px-1 pb-2 text-xs text-foreground/50">
          Personal keys authenticate MCP clients, scripts, and the CLI as you.
          Send one as{` `}
          <code className="rounded bg-muted px-1 py-0.5 text-xs">
            Authorization: Bearer expu_…
          </code>
          {` `}
          or pass it to{` `}
          <code className="rounded bg-muted px-1 py-0.5 text-xs">
            exponential login
          </code>
          {` `}
          via <code className="rounded bg-muted px-1 py-0.5 text-xs">EXP_TOKEN</code>
          .
        </p>
        <div>
          {keys.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No API keys yet. Keys minted by the desktop app or CLI show up
              here too.
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {keys.map((row) => {
                const isDeviceKey = Boolean(
                  row.name?.startsWith(DEVICE_KEY_PREFIX)
                )
                return (
                  <GlassRow key={row.id}>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">
                        {row.name || `Personal key`}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        <code>{keyPreview(row)}</code>
                        {` · created ${formatDate(row.createdAt)} · last used ${
                          row.lastRequest ? formatDate(row.lastRequest) : `never`
                        }`}
                      </div>
                      {isDeviceKey && (
                        <p className="mt-0.5 text-xs text-muted-foreground">
                          Minted automatically by a signed-in device for its
                          coding sessions.
                        </p>
                      )}
                    </div>
                    <Button
                      size="sm"
                      variant="outline"
                      className="shrink-0 text-destructive hover:text-destructive"
                      onClick={() => setRevokeTarget(row)}
                    >
                      Revoke
                    </Button>
                  </GlassRow>
                )
              })}
            </div>
          )}
        </div>
      </div>

      <Dialog
        open={createOpen}
        onOpenChange={(open) => {
          if (!open) closeCreate()
        }}
      >
        <DialogContent>
          {mintedKey === null ? (
            <>
              <DialogHeader>
                <DialogTitle>Create API key</DialogTitle>
                <DialogDescription>
                  The key acts as you with your full team membership. You can
                  revoke it here at any time.
                </DialogDescription>
              </DialogHeader>
              <DialogBody className="space-y-2">
                <Label htmlFor="api-key-name">Name</Label>
                <Input
                  id="api-key-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Personal key"
                  maxLength={180}
                  onKeyDown={(e) => {
                    if (e.key === `Enter`) void handleMint()
                  }}
                />
                {mintError && (
                  <p className="text-sm text-destructive">{mintError}</p>
                )}
              </DialogBody>
              <DialogFooter>
                <DialogCancel variant="outline" onClick={closeCreate} />
                <Button onClick={() => void handleMint()} disabled={minting}>
                  {minting ? `Creating…` : `Create key`}
                </Button>
              </DialogFooter>
            </>
          ) : (
            <>
              <DialogHeader>
                <DialogTitle>Copy your API key</DialogTitle>
                <DialogDescription>
                  This is the only time the full key is shown. Store it
                  somewhere safe — only a hash stays on the server.
                </DialogDescription>
              </DialogHeader>
              <DialogBody className="space-y-2">
                <div className="flex items-center gap-2">
                  <code className="min-w-0 flex-1 overflow-x-auto rounded-md border bg-muted px-3 py-2 text-xs">
                    {mintedKey}
                  </code>
                  <Button
                    size="icon"
                    variant="outline"
                    className="shrink-0"
                    aria-label="Copy API key"
                    onClick={() => void handleCopy()}
                  >
                    {copied ? (
                      <Check className="h-4 w-4" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </Button>
                </div>
              </DialogBody>
              <DialogFooter>
                <Button onClick={closeCreate}>Done</Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null)
        }}
      >
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>Revoke API key</DialogTitle>
            <DialogDescription>
              {revokeIsDeviceKey
                ? `This key was minted by a signed-in device. Revoking it disconnects that device's coding-agent and MCP wiring until it signs in again.`
                : `Anything still using this key stops working immediately. This cannot be undone.`}
            </DialogDescription>
          </DialogHeader>
          <DialogBody>
            <p className="text-sm">
              <span className="font-medium">
                {revokeTarget?.name || `Personal key`}
              </span>
              {` `}
              <code className="text-xs text-muted-foreground">
                {revokeTarget ? keyPreview(revokeTarget) : ``}
              </code>
            </p>
          </DialogBody>
          <DialogFooter>
            <DialogCancel
              variant="outline"
              onClick={() => setRevokeTarget(null)}
            />
            <Button
              variant="destructive"
              onClick={() => void handleRevoke()}
              disabled={revoking}
            >
              {revoking ? `Revoking…` : `Revoke key`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
