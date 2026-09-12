import { useEffect, useState } from "react"
import { authClient } from "@/lib/auth/client"
import { authErrorMessage } from "@/lib/auth/error-messages"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
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
export function PasskeysSection({
  passkeyEnabled,
}: {
  passkeyEnabled: boolean
}) {
  const [rows, setRows] = useState<PasskeyRow[] | null>(null)
  const [loadError, setLoadError] = useState(``)
  const [addOpen, setAddOpen] = useState(false)
  const [name, setName] = useState(``)
  const [adding, setAdding] = useState(false)
  const [addError, setAddError] = useState(``)
  const [removeTarget, setRemoveTarget] = useState<PasskeyRow | null>(null)
  const [removing, setRemoving] = useState(false)

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
    } catch {
      setAddError(`Couldn't add the passkey.`)
    } finally {
      setAdding(false)
    }
  }

  const handleRemove = async () => {
    if (!removeTarget || removing) return
    setRemoving(true)
    try {
      const { error } = await authClient.passkey.deletePasskey({
        id: removeTarget.id,
      })
      if (error) {
        console.error(`[passkeys] delete failed:`, error)
        return
      }
      setRows((prev) => (prev ?? []).filter((row) => row.id !== removeTarget.id))
      setRemoveTarget(null)
    } finally {
      setRemoving(false)
    }
  }

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
                  {row.backedUp
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

      <Dialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove this passkey?</DialogTitle>
            <DialogDescription>
              {`"${removeTarget?.name || `Passkey`}" will no longer sign you in. The
              copy on your device stays until you delete it there.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel variant="outline" onClick={() => setRemoveTarget(null)} />
            <Button
              variant="destructive"
              onClick={() => void handleRemove()}
              disabled={removing}
            >
              {removing ? `Removing…` : `Remove`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
