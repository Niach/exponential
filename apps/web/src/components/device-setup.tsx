// The device-setup block (EXP-1169): the ONE place the app says "get a
// device", byte-identical on its four hosts — the wizard's devices step, the
// join step an invited teammate with no machine gets, the Add device dialog
// and the readiness "Set up a server" fix (which opens that dialog).
//
// Top to bottom: the desktop download card, the server card, then the
// caller's OWN machines. The server card carries a freshly minted one-time
// `EXP_INSTALL_TOKEN` (EXP-1111: `devices.createInstallToken`, 15 minutes,
// reminted on expiry) so the new daemon signs itself in, with the plain
// command as the documented fallback (the CLI then prints a device code).
// That code can be approved right here (the same claim + approve calls as
// /auth/device). The block then watches the synced devices shape and says so
// once a NEW machine of the caller comes online.
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react"
import { useLiveQuery } from "@tanstack/react-db"
import type { Device } from "@/db/schema"
import {
  Button,
  conceptIcon,
  getDeviceIcon,
  GlassRow,
  GlassSectionHeader,
  Input,
  Label,
  LiveDot,
  Pill,
} from "@exp/ui"
import { authClient } from "@/lib/auth/client"
import {
  deviceErrorMessage,
  isCompleteUserCode,
  normalizeUserCode,
} from "@/lib/auth/device-code"
import { deviceCollection } from "@/lib/collections"
import {
  DESKTOP_RELEASES_URL,
  desktopDownloadHref,
} from "@/lib/desktop-download"
import { relativeTime } from "@/components/comment-rows/format"
import {
  composeDeviceList,
  deviceIsMine,
  deviceIsOnline,
  deviceUnauthedAgentIds,
  type SteerDevice,
} from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { GETTING_STARTED_COPY } from "@/components/getting-started/getting-started-copy"
import { ONBOARDING_COPY } from "@/components/onboarding/onboarding-copy"

const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)
const DesktopIcon = conceptIcon(`ui-device`)
const ServerIcon = conceptIcon(`ui-server`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const DownloadIcon = conceptIcon(`ui-download`)
const SignInIcon = conceptIcon(`ui-sign-in`)

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
 * (EXP-697 — shared layout with the IDE's). */
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

// The row's second line (native `deviceStatusLine` parity): a live dot +
// "Online", or the last-seen caption for offline devices. EXP-862: it says
// nothing about sign-ins — the account chips own that.
export function DeviceStatusLine({
  online,
  lastSeenAt,
}: {
  online: boolean
  lastSeenAt: string | null | undefined
}) {
  if (!online) {
    return (
      <div className="truncate text-xs text-muted-foreground">
        {lastSeenAt ? `Last seen ${relativeTime(lastSeenAt)}` : `Offline`}
      </div>
    )
  }
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <LiveDot tone="live" className="size-1.5 shrink-0" />
      <span className="truncate">Online</span>
    </div>
  )
}

/**
 * The first of the caller's machines that came online SINCE the block
 * mounted: a device id the opening snapshot did not have, or one that was
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

/**
 * The caller's OWN machines off the synced devices shape (user-scoped, so
 * they show regardless of the shape rotation a new membership just caused).
 * null while loading: `isReady` is the signal (the use-agents-data idiom), an
 * empty pre-snapshot array must not flash "No devices yet" at someone who has
 * a machine.
 */
export function useOwnDevices(teamId: string): SteerDevice[] | null {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const { data: deviceRows, isReady } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  // 30s tick against the online window (the EXP-153 staleness idiom).
  const now = useNow(30_000)
  return useMemo<SteerDevice[] | null>(() => {
    if (!currentUserId || !isReady || deviceRows === undefined) return null
    return composeDeviceList(
      deviceRows as Device[],
      new Map(),
      now,
      currentUserId,
      teamId
    ).filter(deviceIsMine)
  }, [deviceRows, isReady, now, currentUserId, teamId])
}

interface MintedToken {
  token: string
  expiresAt: number
}

export function DeviceSetup({
  devices,
  origin,
  active = true,
}: {
  /** The caller's OWN devices (synced shape); null while loading. */
  devices: readonly SteerDevice[] | null
  origin: string
  /** False while the host is closed (a dialog): nothing mints or waits. */
  active?: boolean
}) {
  const userAgent = typeof navigator === `undefined` ? `` : navigator.userAgent
  const touchPoints =
    typeof navigator === `undefined` ? undefined : navigator.maxTouchPoints

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
    if (!active) {
      generation.current++
      setMinted(null)
      setMintError(``)
      return
    }
    void mint()
  }, [active, mint])
  // Remint once the token lapses, so the box never shows a dead command.
  useEffect(() => {
    if (!active || !minted) return
    const timer = window.setTimeout(
      () => void mint(),
      Math.max(0, minted.expiresAt - Date.now())
    )
    return () => window.clearTimeout(timer)
  }, [active, minted, mint])

  // ── Approving the CLI's device code here ────────────────────────────────
  const [code, setCode] = useState(``)
  const [codeBusy, setCodeBusy] = useState(false)
  const [codeError, setCodeError] = useState(``)
  const [approved, setApproved] = useState(false)
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (active) return
    setCode(``)
    setCodeError(``)
    setApproved(false)
    setCopied(false)
  }, [active])

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
    if (!active) {
      setBaseline(null)
      return
    }
    if (baseline || devices === null) return
    setBaseline(
      new Map(devices.map((device) => [device.deviceId, deviceIsOnline(device)]))
    )
  }, [active, devices, baseline])
  const arrived =
    active && baseline && devices ? newlyOnlineDevice(baseline, devices) : null

  const tokenSnippet = minted
    ? buildServerInstallSnippet(origin, minted.token)
    : null
  const plainSnippet = buildServerInstallSnippet(origin)

  return (
    <div className="flex flex-col gap-4" data-testid="device-setup">
      <GlassRow className="flex-col items-stretch gap-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <DesktopIcon className="size-4 shrink-0" />
          {GETTING_STARTED_COPY.desktop.title}
        </div>
        <p className="text-sm text-muted-foreground">
          {GETTING_STARTED_COPY.desktop.description}
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" asChild>
            <a
              href={desktopDownloadHref(userAgent, touchPoints)}
              target="_blank"
              rel="noreferrer"
            >
              <DownloadIcon className="mr-1.5 size-4" />
              {GETTING_STARTED_COPY.desktop.action}
            </a>
          </Button>
          <Button size="sm" variant="outline" asChild>
            <a href={DESKTOP_RELEASES_URL} target="_blank" rel="noreferrer">
              All platforms
            </a>
          </Button>
        </div>
      </GlassRow>

      <GlassRow className="flex-col items-stretch gap-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <ServerIcon className="size-4 shrink-0" />
          {GETTING_STARTED_COPY.server.title}
        </div>
        <p className="text-sm text-muted-foreground">
          {GETTING_STARTED_COPY.server.description}
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
      </GlassRow>

      <div>
        <GlassSectionHeader label={ONBOARDING_COPY.devices.yours} />
        {devices === null ? (
          <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
        ) : devices.length === 0 ? (
          <div className="flex items-center gap-2 px-1 py-3 text-xs text-muted-foreground">
            <OfflineIcon className="size-3.5 shrink-0" />
            {ONBOARDING_COPY.devices.none}
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {devices.map((device) => {
              const online = deviceIsOnline(device)
              // The first signed-out agent is what the pill signs in; a
              // device with every agent signed in (or none installed)
              // offers nothing here.
              const signInAgent = deviceUnauthedAgentIds(device)[0]
              const KindIcon = getDeviceIcon(device)
              return (
                <GlassRow key={device.deviceId}>
                  <KindIcon className="size-4 shrink-0 text-foreground/70" />
                  <div className="min-w-0 flex-1">
                    <div className="min-w-0 truncate text-sm font-medium">
                      {device.deviceLabel || device.deviceId}
                    </div>
                    <DeviceStatusLine
                      online={online}
                      lastSeenAt={device.lastSeenAt}
                    />
                  </div>
                  {signInAgent && (
                    <Pill
                      mode="action"
                      onClick={() =>
                        requestAgentLogin({ device, agent: signInAgent })
                      }
                    >
                      <SignInIcon className="size-3" />
                      Sign in
                    </Pill>
                  )}
                </GlassRow>
              )
            })}
          </div>
        )}
        {arrived ? (
          <p
            className="mt-3 flex items-center gap-1.5 text-sm"
            data-testid="add-device-online"
          >
            <CheckIcon className="size-4 shrink-0 text-emerald-500" />
            {`${arrived.deviceLabel || arrived.deviceId} is online`}
          </p>
        ) : (
          (approved || copied) && (
            <p className="mt-3 text-sm text-muted-foreground">
              Waiting for the device to come online…
            </p>
          )
        )}
      </div>
    </div>
  )
}
