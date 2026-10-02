import ExpUI
import ExpCore
import SwiftUI

/// "Add device" (EXP-1169): the device setup block in a sheet, opened by the
/// Devices tab's "Add device" pill and the coding readiness sheet's server
/// fix. No trailing button: a swipe down dismisses, and a machine that signs
/// in meanwhile simply appears in the block's own list.
struct AddDeviceSheet: View {
    let accountId: String

    var body: some View {
        GlassSheetChrome(title: "Add device") {
            // The cards alone: the Devices tab behind this sheet already
            // lists the caller's machines.
            DeviceSetup(accountId: accountId, listsDevices: false, onDevicesChanged: { _ in })
                .padding(16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("add-device-sheet")
    }
}
