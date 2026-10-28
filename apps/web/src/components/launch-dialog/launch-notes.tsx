import {
  FieldError,
  deviceReadinessBlocker,
  deviceReadinessWarning,
} from "@exp/ui"
import { DeviceReadinessNotice } from "@/components/device-readiness-notice"
import {
  MAX_ISSUES_PER_RUN,
  type LaunchComposerModel,
} from "@/hooks/use-launch-composer"

// S-3 (composer-menu.json): NO options line under the card any more:
// Device, Account, Model and Repository are "+" rows. What stays under the
// card are the launch's NOTES (FEED-79: invalid input inline): no desktop
// online, the picked agent not ready on the picked machine (EXP-773) or its
// failing doctor row (EXP-1196), a play button's unavailable machine
// (EXP-836) and the batch guards. Nothing renders while none applies.

export function LaunchNotes({ model }: { model: LaunchComposerModel }) {
  const { launch, candidateDevices } = model
  const { agent, device } = launch

  if (candidateDevices.length === 0) {
    return (
      <p className="px-1 text-xs text-muted-foreground" data-testid="agent-launch-notes">
        No desktop online. Open the Exponential desktop app to start a session.
      </p>
    )
  }

  const notReadyLine = launch.agentNotReady && device && !deviceReadinessBlocker(device.doctor, agent)
  const doctorRow =
    device &&
    (deviceReadinessBlocker(device.doctor, agent) ||
      (!launch.agentNotReady && deviceReadinessWarning(device.doctor)))
  const any =
    notReadyLine ||
    doctorRow ||
    model.deviceRequestNote ||
    model.overCap ||
    model.spansRepos ||
    model.spansBranches ||
    model.costHint
  if (!any) return null

  return (
    <div className="flex flex-col gap-1 px-1 text-xs text-muted-foreground" data-testid="agent-launch-notes">
      {/* EXP-773: a not-ready combination cannot start at all — the
          submit is disabled on the same predicate. EXP-1196: with a doctor
          report the failing ROW renders instead. */}
      {notReadyLine && (
        <span>{`Not ready on ${device.deviceLabel || device.deviceId}. Run the doctor there.`}</span>
      )}
      {/* EXP-836: a play button named a machine this composer cannot start
          on — say which and why, instead of quietly using the default. */}
      {model.deviceRequestNote && (
        <span className="text-amber-500">{model.deviceRequestNote}</span>
      )}
      {/* EXP-1196: the picked device's failing doctor row (Git, else the
          agent's own) with its one action, instead of a sentence. */}
      {device && doctorRow && (
        <DeviceReadinessNotice
          device={device}
          agent={agent}
          notReady={launch.agentNotReady}
          className="rounded-md bg-glass-row"
        />
      )}
      {/* The batch guards are about the PICKED issues (invalid input,
          FEED-79): the field's own rose line, not a toast. */}
      {model.overCap && (
        <FieldError>
          {`At most ${MAX_ISSUES_PER_RUN} issues per session. Split the batch.`}
        </FieldError>
      )}
      {model.spansRepos && (
        <FieldError>Pick issues from a single repository per session.</FieldError>
      )}
      {model.spansBranches && (
        <FieldError>
          Pick issues that branch from a single base branch per session.
        </FieldError>
      )}
      {model.costHint && <p>Large batches are token-expensive.</p>}
    </div>
  )
}
