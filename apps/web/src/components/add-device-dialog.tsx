// The Add device dialog (EXP-697, EXP-1111). Desktop download first, then the
// CLI one-liner — since EXP-1111 carrying a freshly minted one-time
// `EXP_INSTALL_TOKEN` (`devices.createInstallToken`, 15 minutes, reminted on
// expiry) so the new daemon signs itself in, with the plain command as the
// documented fallback (the CLI then prints a device code). That code can be
// approved right here ("Or enter the code the CLI shows", the same claim +
// approve calls as /auth/device). The dialog then watches the synced devices
// shape and says so once a NEW machine of the caller comes online.
import { useCallback, useEffect, useRef, useState, type FormEvent } from "react"
import {
  Button,
  conceptIcon,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@exp/ui"
import { authClient } from "@/lib/auth/client"
import {
  deviceErrorMessage,
  isCompleteUserCode,
  normalizeUserCode,
} from "@/lib/auth/device-code"
import { desktopDownloadHref } from "@/lib/desktop-download"
import { deviceIsOnline, type SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"

const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)

// The install script is served by the CLOUD marketing site for every
// instance — self-hosted deployments ship only the web app (no marketing
// pages), so the one-liner always names the target instance explicitly via
// EXP_INSTANCE and the script itself is identical everywhere. EXP-1111: a
// one-time install token rides along as EXP_INSTALL_TOKEN when there is one.
export function buildServerInstallSnippet(origin: string, token?: string): string {
  const tokenPart = token ? ` EXP_INSTALL_TOKEN=${token}` : ``
  return `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${origin}${tokenPart} sh`
}

/** The same command on two fixed lines (never a horizontal scroll). */
function displayedSnippet(origin: string, token?: string): string {
  const tokenPart = token ? ` \\\n  EXP_INSTALL_TOKEN=${token}` : ``
  return `curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=${origin}${tokenPart} sh`
}

/** The icon-only copy control living INSIDE the install-snippet box
 * (EXP-697 — the add-device dialog, shared layout with the IDE's). */
export function CopyIconButton({
  text,
  onCopied,
}: {
  text: string
  onCopied?: () => void
}) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      variant="glass"
      size="icon-sm"
      className="absolute top-1.5 right-1.5"
      aria-label="Copy install command"
      title="Copy install command"
      onClick={() => {
        void navigator.clipboard.writeText(text)
        setCopied(true)
        onCopied?.()
        window.setTimeout(() => setCopied(false), 1_500)
      }}
    >
      {copied ? (
        <CheckIcon className="size-3.5" />
      ) : (
        <CopyIcon className="size-3.5" />
      )}
    </Button>
  )
}

function SnippetBox({
  display,
  copy,
  onCopied,
  testId,
}: {
  display: string
  copy: string
  onCopied?: () => void
  testId: string
}) {
  return (
    <div className="relative">
      <pre
        className="rounded-md border bg-muted/30 p-3 pr-10 text-left text-xs break-all whitespace-pre-wrap"
        data-testid={testId}
      >
        {display}
      </pre>
      <CopyIconButton text={copy} onCopied={onCopied} />
    </div>
  )
}

/**
 * The first of the caller's machines that came online SINCE the dialog
 * opened: a device id the opening snapshot did not have, or one that was
 * offline then (a reinstall on a known machine keeps its id).
 */
export function newlyOnlineDevice(
  baseline: ReadonlyMap<string, boolean>,
  devices: readonly SteerDevice[]
): SteerDevice | null {
  return (
    devices.find(
      (device) => deviceIsOnline(device) && baseline.get(device.deviceId) !== true
    ) ?? null
  )
}

interface MintedToken {
  token: string
  expiresAt: number
}

export function AddDeviceDialog({
  open,
  onOpenChange,
  devices,
  origin,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The caller's OWN devices (synced shape); null while loading. */
  devices: readonly SteerDevice[] | null
  origin: string
}) {
  const userAgent = typeof navigator === `undefined` ? `` : navigator.userAgent

  // ── The one-time install token ──────────────────────────────────────────
  const [minted, setMinted] = useState<MintedToken | null>(null)
  const [mintError, setMintError] = useState(``)
  // A close (or a newer mint) orphans an in-flight one.
  const generation = useRef(0)
  const mint = useCallback(async () => {
    const mine = ++generation.current
    setMintError(``)
    try {
      const result = await trpc.devices.createInstallToken.mutate()
      if (mine !== generation.current) return
      setMinted({ token: result.token, expiresAt: Date.parse(result.expiresAt) })
    } catch (error) {
      if (mine !== generation.current) return
      setMinted(null)
      setMintError(
        trpcErrorMessage(error, `Couldn't create a one-time install command.`)
      )
    }
  }, [])
  useEffect(() => {
    if (!open) {
      generation.current++
      setMinted(null)
      setMintError(``)
      return
    }
    void mint()
  }, [open, mint])
  // Remint once the token lapses, so the box never shows a dead command.
  useEffect(() => {
    if (!open || !minted) return
    const timer = window.setTimeout(
      () => void mint(),
      Math.max(0, minted.expiresAt - Date.now())
    )
    return () => window.clearTimeout(timer)
  }, [open, minted, mint])

  // ── Approving the CLI's device code here ────────────────────────────────
  const [code, setCode] = useState(``)
  const [codeBusy, setCodeBusy] = useState(false)
  const [codeError, setCodeError] = useState(``)
  const [approved, setApproved] = useState(false)
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (open) return
    setCode(``)
    setCodeError(``)
    setApproved(false)
    setCopied(false)
  }, [open])

  const approve = async (event: FormEvent) => {
    event.preventDefault()
    if (codeBusy || !isCompleteUserCode(code)) return
    setCodeBusy(true)
    setCodeError(``)
    try {
      // GET /api/auth/device claims the code for this session — approve
      // refuses codes nobody claimed (the /auth/device page's two calls).
      const claimed = await authClient.device({ query: { user_code: code } })
      if (claimed.error) {
        setCodeError(deviceErrorMessage(claimed.error))
        return
      }
      const status = claimed.data?.status
      if (status === `approved` || status === `denied`) {
        setCodeError(`That code has already been used. Run the login command again.`)
        return
      }
      const result = await authClient.device.approve({ userCode: code })
      if (result.error) {
        setCodeError(deviceErrorMessage(result.error))
        return
      }
      setApproved(true)
    } catch {
      setCodeError(`Something went wrong. Try again.`)
    } finally {
      setCodeBusy(false)
    }
  }

  // ── Waiting for the new machine ─────────────────────────────────────────
  const [baseline, setBaseline] = useState<ReadonlyMap<string, boolean> | null>(
    null
  )
  useEffect(() => {
    if (!open) {
      setBaseline(null)
      return
    }
    if (baseline || devices === null) return
    setBaseline(
      new Map(devices.map((device) => [device.deviceId, deviceIsOnline(device)]))
    )
  }, [open, devices, baseline])
  const arrived =
    open && baseline && devices ? newlyOnlineDevice(baseline, devices) : null

  const tokenSnippet = minted
    ? buildServerInstallSnippet(origin, minted.token)
    : null
  const plainSnippet = buildServerInstallSnippet(origin)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add device</DialogTitle>
          <DialogDescription>
            To run coding sessions, install the desktop app.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          <Button asChild className="w-fit">
            <a href={desktopDownloadHref(userAgent)} target="_blank" rel="noreferrer">
              Download desktop app
            </a>
          </Button>
          <p className="text-sm text-muted-foreground">
            Or install the Exponential CLI on a server:
          </p>
          {tokenSnippet && minted ? (
            <div className="flex flex-col gap-1.5">
              <SnippetBox
                display={displayedSnippet(origin, minted.token)}
                copy={tokenSnippet}
                onCopied={() => setCopied(true)}
                testId="install-snippet-token"
              />
              <p className="text-xs text-muted-foreground">
                Signs the server in as you. One-time, valid for 15 minutes.
              </p>
            </div>
          ) : mintError ? (
            <p className="text-xs text-destructive">{mintError}</p>
          ) : (
            <p className="text-xs text-muted-foreground">
              Preparing a one-time install command…
            </p>
          )}
          <div className="flex flex-col gap-1.5">
            <p className="text-xs text-muted-foreground">
              Without a token, the CLI shows a code to approve:
            </p>
            <SnippetBox
              display={displayedSnippet(origin)}
              copy={plainSnippet}
              onCopied={() => setCopied(true)}
              testId="install-snippet-plain"
            />
          </div>

          {approved ? (
            <p className="text-sm" data-testid="device-code-approved">
              Code approved. The CLI signs in within a few seconds.
            </p>
          ) : (
            <form onSubmit={(event) => void approve(event)} className="flex flex-col gap-2">
              <Label htmlFor="add-device-code">Or enter the code the CLI shows</Label>
              <div className="flex gap-2">
                <Input
                  id="add-device-code"
                  value={code}
                  onChange={(event) => {
                    setCode(normalizeUserCode(event.target.value))
                    if (codeError) setCodeError(``)
                  }}
                  placeholder="XXXX-XXXX"
                  maxLength={9}
                  autoComplete="off"
                  spellCheck={false}
                  className="font-mono tracking-widest"
                />
                <Button
                  type="submit"
                  disabled={codeBusy || !isCompleteUserCode(code)}
                >
                  Approve
                </Button>
              </div>
              {codeError && <p className="text-sm text-destructive">{codeError}</p>}
            </form>
          )}

          {arrived ? (
            <p
              className="flex items-center gap-1.5 text-sm"
              data-testid="add-device-online"
            >
              <CheckIcon className="size-4 shrink-0 text-emerald-500" />
              {`${arrived.deviceLabel || arrived.deviceId} is online`}
            </p>
          ) : (
            (approved || copied) && (
              <p className="text-sm text-muted-foreground">
                Waiting for the device to come online…
              </p>
            )
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
