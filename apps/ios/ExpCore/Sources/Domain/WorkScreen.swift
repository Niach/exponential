import Foundation

// NEW-59: THE work screen, the display coding agents drive with computer use.
// A port of the TS reference `packages/domain-contract/src/work-screen.ts`
// (desktop computer crate `work_screen`, Android `domain/WorkScreen.kt`),
// byte-locked by `fixtures/work-screen.json` (`copy`, `testIds` and
// `optionCases`, replayed by `WorkScreenTests`). The phones only show and edit
// the device's setting (`launchDefaults.computerUseDisplay`): the gate and the
// overlay rule are desktop-only. The fixture's `$comment` holds the rules.

public enum WorkScreen {
    public enum Copy {
        public static let row = "Work screen"
        public static let auto = "Auto"
        public static let display = "Display {number}"
        public static let primary = "Primary"
        public static let notConnected = "Not connected"

        // The `wrong_display` refusals are the desktop hub's alone (not mirrored).
    }

    public enum TestIds {
        public static let row = "device-settings-work-screen"
    }

    /// One display as the device reports it in `doctor.displays` (OS order,
    /// pixels). Decoded leniently by `DeviceDoctor`: an entry missing a field
    /// drops alone.
    public struct Display: Decodable, Equatable, Sendable {
        public let number: Int
        public let width: Int
        public let height: Int
        public let primary: Bool

        public init(number: Int, width: Int, height: Int, primary: Bool) {
            self.number = number
            self.width = width
            self.height = height
            self.primary = primary
        }
    }

    /// One picker option; `value` = the stored `computerUseDisplay` (1-based),
    /// nil = Auto.
    public struct Option: Equatable, Hashable, Sendable {
        public let value: Int?
        public let label: String

        public init(value: Int?, label: String) {
            self.value = value
            self.label = label
        }
    }

    private static let separator = " · "

    private static func displayLabel(_ number: Int) -> String {
        Copy.display.replacingOccurrences(of: "{number}", with: String(number))
    }

    /// The Work screen picker's options: Auto, the reported displays, then a
    /// stored display the device no longer reports.
    public static func options(displays: [Display]?, stored: Int?) -> [Option] {
        var out = [Option(value: nil, label: Copy.auto)]
        for display in displays ?? [] {
            var label = "\(displayLabel(display.number))\(separator)\(display.width) × \(display.height)"
            if display.primary { label += "\(separator)\(Copy.primary)" }
            out.append(Option(value: display.number, label: label))
        }
        if let stored, !out.contains(where: { $0.value == stored }) {
            out.append(Option(value: stored, label: "\(displayLabel(stored))\(separator)\(Copy.notConnected)"))
        }
        return out
    }
}
