import { useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import type { Device } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { deviceCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import {
  composeDeviceList,
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  deviceUnauthedAgentIds,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  DESKTOP_RELEASES_URL,
  desktopDownloadHref,
} from "@/lib/desktop-download"
import {
  buildServerInstallSnippet,
  CopyIconButton,
  DeviceStatusLine,
} from "@/components/my-machines"
import { DeviceSettingsDialog } from "@/components/device-settings-dialog"
import { GETTING_STARTED_COPY } from "@/components/getting-started/getting-started-copy"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { ONBOARDING_COPY } from "@/components/onboarding/onboarding-copy"
import { StepCard, stepAdvanceLabel } from "@/components/onboarding/step-card"

const DevicesIcon = conceptIcon(`nav-devices`)
const DesktopIcon = conceptIcon(`ui-device`)
const ServerIcon = conceptIcon(`ui-server`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const DownloadIcon = conceptIcon(`ui-download`)
const SignInIcon = conceptIcon(`ui-sign-in`)

// Step 4, the last one (EXP-725): everything that means leaving this screen
// for another machine — download the desktop app, install the CLI daemon on
// a server, sign the agents in — so it comes after the board and is always
// skippable. The two cards reuse the getting-started checklist's copy and
// the add-device dialog's install box (`my-machines.tsx`); the device rows
// are the caller's own machines off the synced devices shape (user-scoped,
// so they show regardless of the shape rotation the new team just caused),
// each opening the device-settings dialog whose agent tabs carry the remote
// `agent_login` button (EXP-484/688).
export function DevicesStep({
  teamId,
  onNext,
}: {
  teamId: string
  onNext: () => void
}) {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const { data: deviceRows, isReady } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  // 30s tick against the online window (the EXP-153 staleness idiom).
  const now = useNow(30_000)
  const devices = useMemo<SteerDevice[] | null>(() => {
    // `isReady` is the loading signal (the use-agents-data idiom): an empty
    // pre-snapshot array must not flash "No devices yet" at someone who has
    // a machine.
    if (!currentUserId || !isReady || deviceRows === undefined) return null
    return composeDeviceList(
      deviceRows as Device[],
      new Map(),
      now,
      currentUserId,
      teamId
    ).filter(deviceIsMine)
  }, [deviceRows, isReady, now, currentUserId, teamId])

  const [settingsTargetId, setSettingsTargetId] = useState<string | null>(null)
  const settingsTarget =
    devices?.find((device) => device.deviceId === settingsTargetId) ?? null

  const origin =
    typeof window === `undefined`
      ? `https://app.exponential.at`
      : window.location.origin
  const snippet = buildServerInstallSnippet(origin)
  const downloadHref =
    typeof navigator === `undefined`
      ? desktopDownloadHref(``)
      : desktopDownloadHref(navigator.userAgent, navigator.maxTouchPoints)

  return (
    <StepCard
      icon={DevicesIcon}
      title={ONBOARDING_COPY.devices.title}
      subtitle={ONBOARDING_COPY.devices.subtitle}
    >
      <div className="space-y-4 p-6">
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
              <a href={downloadHref} target="_blank" rel="noreferrer">
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
          <div className="relative">
            <pre className="rounded-md border bg-muted/30 p-3 pr-10 text-left text-xs whitespace-pre-wrap">
              {`curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=${origin} sh`}
            </pre>
            <CopyIconButton text={snippet} />
          </div>
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
                const unauthed = deviceUnauthedAgentIds(device)
                const signInNeeded =
                  online && !deviceHasRunnableAgent(device) && unauthed.length > 0
                const KindIcon =
                  device.kind === `server` ? ServerIcon : DesktopIcon
                return (
                  <GlassRow key={device.deviceId}>
                    <KindIcon className="size-4 shrink-0 text-foreground/70" />
                    <div className="min-w-0 flex-1">
                      <div className="min-w-0 truncate text-sm font-medium">
                        {device.deviceLabel || device.deviceId}
                      </div>
                      <DeviceStatusLine
                        online={online}
                        signInNeeded={signInNeeded}
                        unauthed={unauthed}
                        lastSeenAt={device.lastSeenAt}
                      />
                    </div>
                    <Pill
                      mode="action"
                      onClick={() => setSettingsTargetId(device.deviceId)}
                    >
                      <SignInIcon className="size-3" />
                      Agents
                    </Pill>
                  </GlassRow>
                )
              })}
            </div>
          )}
        </div>

        <div className="flex justify-end">
          <Button
            variant={devices && devices.length > 0 ? `default` : `outline`}
            onClick={onNext}
            data-testid="onboarding-advance"
          >
            {stepAdvanceLabel(
              (devices?.length ?? 0) > 0,
              ONBOARDING_COPY.nav
            )}
          </Button>
        </div>
      </div>

      <DeviceSettingsDialog
        device={settingsTarget}
        open={settingsTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSettingsTargetId(null)
        }}
      />
    </StepCard>
  )
}
