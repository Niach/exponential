import { useEffect, useState } from "react"
import { Check, Copy, LoaderCircle } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { promptActions, WEB_PROMPTS } from "@/lib/prompts"
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
  Prompt,
  EMPTY_SCOPE_SELECTION,
  ScopePicker,
  effectiveScopeSelection,
  hasScopeSelection,
  scopeCaption,
  type ScopePickerTeam,
  type ScopeSelection,
} from "@exp/ui"

type ApiKeyRow = Awaited<
  ReturnType<typeof trpc.users.listPersonalApiKeys.query>
>[`keys`][number]

// Desktop/CLI sign-ins auto-mint their hidden key under this name prefix
// (crates/api device_key_name). EXP-1054: those rows are the keys a signed-in
// device minted for its coding runs' MCP wiring (crates/api token_store),
// never shown by their token. Revoking one DISCONNECTS those runs; the device
// itself stays signed in (its login is a separate session token) and mints a
// fresh key on its next sign-in or regenerate. Everything else is an API key
// for scripts and MCP clients.
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
  // "Mar 4, 2026" — the IDE's api_keys.rs prints the same shape.
  return Number.isNaN(date.getTime())
    ? `–`
    : date.toLocaleDateString(`en-US`, {
        month: `short`,
        day: `numeric`,
        year: `numeric`,
      })
}

function keyPreview(row: ApiKeyRow): string {
  // Better Auth stores the visible prefix chars in `start`; `prefix` is the
  // static expu_ marker. Show whatever is available without leaking length.
  if (row.start) return `${row.start}…`
  if (row.prefix) return `${row.prefix}…`
  return `expu_…`
}

// Self-service personal API keys (EXP-238). An unscoped key acts as the
// signed-in user everywhere a session does: the MCP endpoint, tRPC, sync — and
// the CLI's EXP_TOKEN. FEED-76: the mint dialog offers the OAuth consent
// screen's team/board selection (`ScopePicker`); a scoped key is MCP-only and
// confined to its pick, and its row wears the pick as a caption. The raw key
// exists client-side only in the mint dialog.
export function ApiKeysSection({ initialKeys }: { initialKeys: ApiKeyRow[] }) {
  const [keys, setKeys] = useState<ApiKeyRow[]>(initialKeys)
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState(``)
  const [tree, setTree] = useState<ScopePickerTeam[] | null>(null)
  const [treeError, setTreeError] = useState(``)
  const [selection, setSelection] = useState<ScopeSelection>(
    EMPTY_SCOPE_SELECTION
  )
  const [minting, setMinting] = useState(false)
  const [mintError, setMintError] = useState(``)
  const [mintedKey, setMintedKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyRow | null>(null)
  const [revoking, setRevoking] = useState(false)

  const sessions = keys.filter(isDeviceKey)
  const apiKeys = keys.filter((row) => !isDeviceKey(row))

  // The scope tree (the member's teams + live boards) loads once per open.
  useEffect(() => {
    if (!createOpen || tree) return
    let cancelled = false
    trpc.mcpGrants.scopeTree
      .query()
      .then((scopes) => {
        if (!cancelled) setTree(scopes.teams)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setTreeError(
          e instanceof Error && e.message
            ? e.message
            : `Couldn't load your teams.`
        )
      })
    return () => {
      cancelled = true
    }
  }, [createOpen, tree])

  const closeCreate = () => {
    setCreateOpen(false)
    setName(``)
    setMintError(``)
    setMintedKey(null)
    setCopied(false)
    setSelection(EMPTY_SCOPE_SELECTION)
  }

  const canMint = hasScopeSelection(selection) && (selection.allTeams || tree !== null)

  const handleMint = async () => {
    if (minting || !canMint) return
    setMinting(true)
    setMintError(``)
    try {
      // "Everything" sends no scope: the key stays an ordinary full key.
      const scope = selection.allTeams
        ? undefined
        : effectiveScopeSelection(tree ?? [], selection)
      const created = await trpc.users.mintPersonalApiKey.mutate({
        ...(name.trim() ? { name: name.trim() } : {}),
        ...(scope ? { scope } : {}),
      })
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
          scope: created.scope,
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
  const revokeCopy =
    revokeTarget && revokeIsSession
      ? WEB_PROMPTS.disconnectDeviceKey(deviceName(revokeTarget))
      : WEB_PROMPTS.revokeApiKey(
          revokeTarget?.name || `Personal key`,
          revokeTarget ? keyPreview(revokeTarget) : ``
        )

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
                  Disconnect
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
                  <div
                    className="text-xs text-muted-foreground"
                    data-slot="api-key-scope"
                  >
                    {scopeCaption(row.scope)}
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
                  For scripts and MCP clients. The key acts as you within the
                  teams and boards you choose; revoke it here at any time.
                </DialogDescription>
              </DialogHeader>
              <DialogBody className="space-y-4">
                <div className="space-y-2">
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
                </div>
                <div className="space-y-2">
                  <Label>Access</Label>
                  {treeError ? (
                    <p className="text-sm text-destructive">{treeError}</p>
                  ) : !tree && !selection.allTeams ? (
                    <div className="flex justify-center py-4">
                      <LoaderCircle className="h-5 w-5 animate-spin text-muted-foreground" />
                    </div>
                  ) : (
                    <ScopePicker
                      tree={tree ?? []}
                      value={selection}
                      onChange={setSelection}
                      idPrefix="api-key"
                    />
                  )}
                  {!selection.allTeams && (
                    <p className="text-xs text-muted-foreground">
                      A scoped key works with the MCP endpoint only and cannot
                      be re-scoped later — create a new key to widen it.
                    </p>
                  )}
                </div>
                {mintError && (
                  <p className="text-sm text-destructive">{mintError}</p>
                )}
              </DialogBody>
              <DialogFooter>
                <DialogCancel variant="outline" onClick={closeCreate} />
                <Button
                  onClick={() => void handleMint()}
                  disabled={minting || !canMint}
                >
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

      <Prompt
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null)
        }}
        busy={revoking}
        title={revokeCopy.title}
        body={revokeCopy.body}
        actions={promptActions(revokeCopy, {
          [revokeIsSession ? `disconnect` : `revoke`]: {
            busy: revoking,
            onSelect: () => handleRevoke(),
          },
        })}
      />
    </div>
  )
}
