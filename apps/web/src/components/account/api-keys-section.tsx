import { useState } from "react"
import { Check, Copy } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import {
  Button,
  Pill,
  GlassSectionHeader,
  ListRow,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@exp/ui"

type ApiKeyRow = Awaited<
  ReturnType<typeof trpc.users.listPersonalApiKeys.query>
>[`keys`][number]

// Desktop/CLI sign-ins auto-mint their hidden key under this name prefix
// (crates/api device_key_name). EXP-1054: those rows are LOGIN SESSIONS — a
// signed-in device, never shown by its token — and "revoking" one logs that
// device out. Everything else is an API key for scripts and MCP clients.
const DEVICE_KEY_PREFIX = `Device: `

function isDeviceKey(row: ApiKeyRow): boolean {
  return Boolean(row.name?.startsWith(DEVICE_KEY_PREFIX))
}

/** The device's own name — the `Device: ` mint prefix stripped. */
function deviceName(row: ApiKeyRow): string {
  const name = row.name?.slice(DEVICE_KEY_PREFIX.length).trim()
  return name || `Device`
}

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

  const sessions = keys.filter(isDeviceKey)
  const apiKeys = keys.filter((row) => !isDeviceKey(row))

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

  // One mutation for both lists: a device's login session IS its hidden key.
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

  const revokeIsSession = revokeTarget !== null && isDeviceKey(revokeTarget)

  return (
    <div className="space-y-6">
      <div>
        <GlassSectionHeader label="Login sessions" />
        {sessions.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No signed-in devices.
          </p>
        ) : (
          // EXP-862: flat rows under the band, never one card per row.
          <div className="flex flex-col">
            {sessions.map((row) => (
              <ListRow key={row.id}>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">
                    {deviceName(row)}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {`Signed in ${formatDate(row.createdAt)} · last active ${
                      row.lastRequest ? formatDate(row.lastRequest) : `never`
                    }`}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  className="shrink-0"
                  onClick={() => setRevokeTarget(row)}
                >
                  Log out
                </Button>
              </ListRow>
            ))}
          </div>
        )}
      </div>

      <div>
        <GlassSectionHeader
          label="API keys"
          trailing={
            <Pill mode="action" onClick={() => setCreateOpen(true)}>
              Create key
            </Pill>
          }
        />
        {apiKeys.length === 0 ? (
          <p className="text-sm text-muted-foreground">No API keys.</p>
        ) : (
          <div className="flex flex-col">
            {apiKeys.map((row) => (
              <ListRow key={row.id}>
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
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  className="shrink-0 text-destructive hover:text-destructive"
                  onClick={() => setRevokeTarget(row)}
                >
                  Revoke
                </Button>
              </ListRow>
            ))}
          </div>
        )}
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
                  For scripts and MCP clients. The key acts as you with your
                  full team membership; revoke it here at any time.
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
            <DialogTitle>
              {revokeIsSession ? `Log out device` : `Revoke API key`}
            </DialogTitle>
            <DialogDescription>
              {revokeIsSession
                ? `The desktop app or CLI on this device signs out. Its coding sessions and MCP wiring stop until it signs in again.`
                : `Anything still using this key stops working immediately. This cannot be undone.`}
            </DialogDescription>
          </DialogHeader>
          <DialogBody>
            <p className="text-sm">
              <span className="font-medium">
                {revokeTarget
                  ? revokeIsSession
                    ? deviceName(revokeTarget)
                    : revokeTarget.name || `Personal key`
                  : ``}
              </span>
              {!revokeIsSession && revokeTarget && (
                <>
                  {` `}
                  <code className="text-xs text-muted-foreground">
                    {keyPreview(revokeTarget)}
                  </code>
                </>
              )}
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
              {revoking
                ? revokeIsSession
                  ? `Logging out…`
                  : `Revoking…`
                : revokeIsSession
                  ? `Log out`
                  : `Revoke key`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
