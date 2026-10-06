import { useEffect, useState } from "react"
import { authClient } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { promptActions, removePasskeyPrompt } from "@/lib/prompts"
import { authErrorMessage } from "@/lib/auth/error-messages"
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
} from "@exp/ui"

// The plugin's list endpoint returns the whole row; only these are rendered.
type PasskeyRow = {
  id: string
  name?: string | null
  deviceType?: string | null
  backedUp?: boolean | null
  createdAt?: Date | string | null
}

function formatDate(value: Date | string | null | undefined): string {
  if (!value) return `–`
  const date = value instanceof Date ? value : new Date(value)
  return Number.isNaN(date.getTime()) ? `–` : date.toLocaleDateString()
}

// A sensible default label from the user agent so the list stays readable
// without typing — the dialog lets the user overwrite it.
function suggestedPasskeyName(): string {
  if (typeof navigator === `undefined`) return `Passkey`
  const ua = navigator.userAgent
  if (/iPhone|iPad/.test(ua)) return `iPhone`
  if (/Android/.test(ua)) return `Android phone`
  if (/Macintosh/.test(ua)) return `Mac`
  if (/Windows/.test(ua)) return `Windows PC`
  if (/Linux/.test(ua)) return `Linux`
  return `Passkey`
}

// Better Auth's fresh-session guard on passkey registration answers this code;
// the only cure is a new sign-in.
const NOT_FRESH_CODE = `SESSION_NOT_FRESH`

// Account-level passkey management (EXP-857): the registration side of
// "Login with passkey". A passkey is bound to this instance's hostname, so
// one registered here works on every client that talks to the same host
// (web, desktop via the browser handoff, iOS and Android natively).
// EXP-1126: lives under Settings › Account, right below Sign-in methods
// (moved back from Security, which keeps the API keys).
export function PasskeysSection({
  passkeyEnabled,
  waysIn,
  onChanged,
}: {
  passkeyEnabled: boolean
  // EXP-1209: `users.signInMethods.waysIn`; at 1 the last passkey's Remove is
  // disabled up front, like the provider rows above.
  waysIn: number
  // EXP-1126: the Sign-in methods band above counts passkeys as ways in.
  onChanged?: () => void
}) {
  const [rows, setRows] = useState<PasskeyRow[] | null>(null)
  const [loadError, setLoadError] = useState(``)
  const [addOpen, setAddOpen] = useState(false)
  const [name, setName] = useState(``)
  const [adding, setAdding] = useState(false)
  const [addError, setAddError] = useState(``)
  const [removeTarget, setRemoveTarget] = useState<PasskeyRow | null>(null)
  const [removing, setRemoving] = useState(false)
  const [removeError, setRemoveError] = useState(``)

  const refresh = async () => {
    try {
      const { data, error } = await authClient.passkey.listUserPasskeys()
      if (error) {
        setLoadError(authErrorMessage(error, `Couldn't load your passkeys.`))
        setRows([])
        return
      }
      setLoadError(``)
      setRows((data ?? []) as PasskeyRow[])
    } catch {
      setLoadError(`Couldn't load your passkeys.`)
      setRows([])
    }
  }

  useEffect(() => {
    if (!passkeyEnabled) return
    void refresh()
  }, [passkeyEnabled])

  const openAdd = () => {
    setName(suggestedPasskeyName())
    setAddError(``)
    setAddOpen(true)
  }

  const closeAdd = () => {
    setAddOpen(false)
    setAddError(``)
  }

  const handleAdd = async () => {
    if (adding) return
    setAdding(true)
    setAddError(``)
    try {
      const { error } = await authClient.passkey.addPasskey({
        name: name.trim() || suggestedPasskeyName(),
      })
      if (error) {
        setAddError(
          (error as { code?: string }).code === NOT_FRESH_CODE
            ? `Sign in again to add a passkey (your current session is older than a day).`
            : authErrorMessage(error, `Couldn't add the passkey.`)
        )
        return
      }
      closeAdd()
      await refresh()
      onChanged?.()
    } catch {
      setAddError(`Couldn't add the passkey.`)
    } finally {
      setAdding(false)
    }
  }

  // EXP-1126: removal goes through tRPC so the last-way-in rule
  // (lib/auth/sign-in-methods.ts) answers with its message instead of a
  // silent console error.
  const handleRemove = async () => {
    if (!removeTarget || removing) return
    setRemoving(true)
    setRemoveError(``)
    try {
      await trpc.users.deletePasskey.mutate({ id: removeTarget.id })
      setRows((prev) => (prev ?? []).filter((row) => row.id !== removeTarget.id))
      setRemoveTarget(null)
      onChanged?.()
    } catch (err) {
      setRemoveError(
        err instanceof Error ? err.message : `Couldn't remove the passkey.`
      )
    } finally {
      setRemoving(false)
    }
  }

  const removeCopy = removePasskeyPrompt(removeTarget?.name || `Passkey`)
  const onlyWayIn = waysIn <= 1

  return (
    <div>
      <GlassSectionHeader
        label="Passkeys"
        trailing={
          passkeyEnabled ? (
            <Pill mode="action" onClick={openAdd}>
              Add passkey
            </Pill>
          ) : undefined
        }
      />
      {!passkeyEnabled ? (
        <p className="text-sm text-muted-foreground">
          Passkeys need an https instance address. Ask whoever runs this
          instance to serve it over https.
        </p>
      ) : rows === null ? (
        <p className="text-sm text-muted-foreground">Loading…</p>
      ) : loadError ? (
        <p className="text-sm text-destructive">{loadError}</p>
      ) : rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">No passkeys yet.</p>
      ) : (
        // EXP-862: flat rows under the band, never one card per passkey.
        <div className="flex flex-col">
          {rows.map((row) => (
            <ListRow key={row.id}>
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">
                  {row.name || `Passkey`}
                </div>
                <div className="text-xs text-muted-foreground">
                  {`added ${formatDate(row.createdAt)}`}
                  {onlyWayIn
                    ? ` · your only way to sign in`
                    : row.backedUp
                      ? ` · synced across your devices`
                      : row.deviceType === `singleDevice`
                        ? ` · this device only`
                        : ``}
                </div>
              </div>
              <Button
                size="sm"
                variant="outline"
                className="shrink-0 text-destructive hover:text-destructive"
                disabled={onlyWayIn}
                onClick={() => setRemoveTarget(row)}
              >
                Remove
              </Button>
            </ListRow>
          ))}
        </div>
      )}

      <Dialog
        open={addOpen}
        onOpenChange={(open) => {
          if (!open) closeAdd()
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add a passkey</DialogTitle>
            <DialogDescription>
              Your browser will ask you to confirm with Face ID, Touch ID, a
              PIN or a security key. Name it after the device so you can tell
              them apart later.
            </DialogDescription>
          </DialogHeader>
          <DialogBody className="space-y-2">
            <Label htmlFor="passkey-name">Name</Label>
            <Input
              id="passkey-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="MacBook"
              maxLength={80}
              onKeyDown={(e) => {
                if (e.key === `Enter`) void handleAdd()
              }}
            />
            {addError && <p className="text-sm text-destructive">{addError}</p>}
          </DialogBody>
          <DialogFooter>
            <DialogCancel variant="outline" onClick={closeAdd} />
            <Button onClick={() => void handleAdd()} disabled={adding}>
              {adding ? `Waiting for your device…` : `Add passkey`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Prompt
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveTarget(null)
        }}
        busy={removing}
        title={removeCopy.title}
        body={removeCopy.body}
        actions={promptActions(removeCopy, {
          remove: { busy: removing, onSelect: () => handleRemove() },
        })}
      >
        {removeError ? (
          <p className="text-sm text-destructive">{removeError}</p>
        ) : null}
      </Prompt>
    </div>
  )
}
