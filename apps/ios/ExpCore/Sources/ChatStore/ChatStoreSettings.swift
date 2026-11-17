import Foundation

/// #EDE-114 THE STORAGE SETTING (chat-store.json, per device, phones):
/// `Keep tool steps` = 7 days, 30 days (default), 90 days, 1 year, Forever
/// (`null`). Prose and messages are kept regardless; a shorter window applies
/// at once. Stored in the standard UserDefaults (this device only).
public enum KeepToolSteps {
    public static let label = "Keep tool steps"
    /// The phones' default window (`constants.retentionDays.phone`).
    public static let defaultDays: UInt32 = 30
    /// The options in order; nil = Forever.
    public static let options: [UInt32?] = [7, 30, 90, 365, nil]
    static let defaultsKey = "chatStore.keepToolStepsDays"
    /// What the defaults hold for Forever (an absent key = the default).
    static let foreverValue = 0

    public static func optionLabel(_ days: UInt32?) -> String {
        switch days {
        case nil: "Forever"
        case 365?: "1 year"
        case let days?: "\(days) days"
        }
    }

    /// The window this device keeps tool steps for (nil = forever).
    public static func windowDays(_ defaults: UserDefaults = .standard) -> UInt32? {
        guard let stored = defaults.object(forKey: defaultsKey) as? Int else { return defaultDays }
        if stored == foreverValue { return nil }
        return options.contains(UInt32(clamping: stored)) ? UInt32(clamping: stored) : defaultDays
    }

    public static func setWindowDays(_ days: UInt32?, _ defaults: UserDefaults = .standard) {
        defaults.set(days.map(Int.init) ?? foreverValue, forKey: defaultsKey)
    }
}
