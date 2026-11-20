import Foundation

// NEW-59: THE work screen, the display coding agents drive with computer use.
// A port of the TS reference `packages/domain-contract/src/work-screen.ts`
// (desktop epilot crate `work_screen`, Android `domain/WorkScreen.kt`),
// byte-locked by `fixtures/work-screen.json` (`copy`, `testIds` and
// `optionCases`, #EXP-1514 `space`, `spaceOptionCases` and `spaceKeyCases`,
// replayed by `WorkScreenTests`). The phones only show and edit the device's
// setting (`launchDefaults.computerUseDisplayId`): the gate and the overlay
// rule are desktop-only. The fixture's `$comment` holds the rules.

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

    /// #EXP-1514: the reserved pick `value` (agents work in an agent space
    /// only) and the picker's words for it, apart from `Copy` (the fixture's
    /// `space`). iOS binds the stored value itself, so no picker keys.
    public enum Space {
        public static let value = "space"
        public static let label = "Agent space"
        public static let only = "Agent space only"
    }

    /// device-doctor.json's agent space row (#EXP-1514).
    public static let agentSpaceItem = "agent_space"

    /// #EXP-1514: whether the device offers an agent space, read by the
    /// picker: its doctor carries the `agent_space` item in state `ok`.
    public static func spaceOffered(_ doctor: DeviceDoctor?) -> Bool {
        doctor?.items.contains { $0.key == agentSpaceItem && $0.state == "ok" } ?? false
    }

    /// One display as the device reports it in `doctor.displays` (OS order,
    /// pixels). Decoded leniently by `DeviceDoctor`: an entry missing a field
    /// (an `id` included, #FEED-161) drops alone.
    public struct Display: Decodable, Equatable, Sendable {
        /// #FEED-161: stable across reconnects and reorders (the stored value).
        public let id: String
        /// 1-based position, for the label only.
        public let number: Int
        public let width: Int
        public let height: Int
        public let primary: Bool

        public init(id: String, number: Int, width: Int, height: Int, primary: Bool) {
            self.id = id
            self.number = number
            self.width = width
            self.height = height
            self.primary = primary
        }
    }

    /// One picker option; `value` = the stored `computerUseDisplayId` (the
    /// display's `id`), nil = Auto.
    public struct Option: Equatable, Hashable, Sendable {
        public let value: String?
        public let label: String

        public init(value: String?, label: String) {
            self.value = value
            self.label = label
        }
    }

    private static let separator = " · "

    private static func displayLabel(_ number: Int) -> String {
        Copy.display.replacingOccurrences(of: "{number}", with: String(number))
    }

    /// The Work screen picker's options: Auto, the agent space, the reported
    /// displays, then a stored display the device no longer reports.
    /// #EXP-1514: while the device offers a space (`spaceOffered`) Auto says
    /// it lands there and Agent space only follows it; a stored space stays
    /// listed in that place, never Not connected.
    public static func options(displays: [Display]?, stored: String?, spaceOffered: Bool = false) -> [Option] {
        let auto = spaceOffered ? "\(Copy.auto)\(separator)\(Space.label)" : Copy.auto
        var out = [Option(value: nil, label: auto)]
        if spaceOffered || stored == Space.value {
            out.append(Option(value: Space.value, label: Space.only))
        }
        for display in displays ?? [] {
            var label = "\(displayLabel(display.number))\(separator)\(display.width) × \(display.height)"
            if display.primary { label += "\(separator)\(Copy.primary)" }
            out.append(Option(value: display.id, label: label))
        }
        if let stored, !out.contains(where: { $0.value == stored }) {
            out.append(Option(value: stored, label: Copy.notConnected))
        }
        return out
    }
}
