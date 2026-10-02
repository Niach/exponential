import { conceptIcon, Button } from "@exp/ui"
import { AgentLoginDialogHost } from "@/components/agent-login-dialog"
import {
  DeviceSetup,
  OwnDevicesList,
  useOwnDevices,
} from "@/components/device-setup"
import { ONBOARDING_COPY } from "@/components/onboarding/onboarding-copy"
import { StepCard, stepAdvanceLabel } from "@/components/onboarding/step-card"

const DevicesIcon = conceptIcon(`nav-devices`)

// The devices step (EXP-725): everything that means leaving this screen for
// another machine — download the desktop app, install the CLI daemon on a
// server, sign the agents in — so it comes last and is always skippable.
// EXP-1169: it has two entrances, the wizard's step 4 after the board and
// the join step an invited teammate with no machine of their own gets
// (`routes/invite/$token.tsx`). The content is the shared device-setup block
// (`device-setup.tsx`), the same one the Add device dialog renders, plus the
// caller's own machines, which only this step lists (the dialog opens over a
// page that already shows them). Neither
// entrance sits under the team route, so the step mounts its own host for
// the block's "Sign in" pill.
export function DevicesStep({
  teamId,
  onNext,
}: {
  teamId: string
  onNext: () => void
}) {
  const devices = useOwnDevices(teamId)
  const origin =
    typeof window === `undefined`
      ? `https://app.exponential.at`
      : window.location.origin

  return (
    <StepCard
      icon={DevicesIcon}
      title={ONBOARDING_COPY.devices.title}
      subtitle={ONBOARDING_COPY.devices.subtitle}
    >
      <div className="space-y-4 p-6">
        <DeviceSetup origin={origin} />
        <OwnDevicesList devices={devices} />

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
      <AgentLoginDialogHost />
    </StepCard>
  )
}
