import Foundation

/// VAPP-4 spike: how the kitchen sink is opened directly. Capture/test only:
/// every switch here needs `-uiTesting` on the command line.
///
/// - `-uiTestingScreen kitchen-sink` / `kitchen-sink-bench`: open the screen
///   (pushed onto the signed-in navigator, or shown at the root when nobody is
///   signed in, so the spike never needs a backend).
/// - `-uiTestingRTL`: the RTL run (tree `direction: rtl` + a right-to-left
///   SwiftUI environment).
/// - `-vappNoUnflip`: skip the RTL un-flip (to observe SwiftUI's own mirroring).
enum VappSpikeLaunch {
    static let benchNodes = 200

    static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains("-uiTesting")
    }

    /// nil = no direct open. `.some(nil)` = the kitchen sink, `.some(200)` = the bench.
    static var directRoute: Int?? {
        let arguments = ProcessInfo.processInfo.arguments
        guard isUITesting,
              let keyIndex = arguments.firstIndex(of: "-uiTestingScreen"),
              arguments.index(after: keyIndex) < arguments.endIndex
        else { return nil }
        switch arguments[arguments.index(after: keyIndex)] {
        case "kitchen-sink": return .some(nil)
        case "kitchen-sink-bench": return .some(benchNodes)
        default: return nil
        }
    }

    static var rtl: Bool {
        isUITesting && ProcessInfo.processInfo.arguments.contains("-uiTestingRTL")
    }

    static var unflipsRTL: Bool {
        !ProcessInfo.processInfo.arguments.contains("-vappNoUnflip")
    }
}
