import ExpUI
import ExpCore
import SwiftUI

/// The devices step (EXP-725, EXP-1169): header + the device setup block +
/// the Skip for now / Continue button. ONE implementation for the wizard's
/// step 4 and the join step every invite accept surface shows a caller who
/// owns no machine (the wizard's join path, the invite deep link, the team
/// setup sheet).
struct DevicesStepView: View {
    let accountId: String
    /// Skip for now / Continue. The host decides where it leads.
    let onAdvance: () -> Void

    /// At least one of the caller's OWN machines is registered.
    @State private var hasOwnDevice = false

    var body: some View {
        VStack(spacing: 0) {
            OnboardingStepHeader(
                title: OnboardingCopy.devicesTitle,
                subtitle: OnboardingCopy.devicesSubtitle
            )

            DeviceSetup(
                accountId: accountId,
                onDevicesChanged: { hasOwnDevice = $0 }
            )

            OnboardingAdvanceButton(done: hasOwnDevice, action: onAdvance)
        }
    }
}
