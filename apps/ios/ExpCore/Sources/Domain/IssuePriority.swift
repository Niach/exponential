import Foundation

public enum IssuePriority: String, CaseIterable, Codable, Identifiable, Sendable {
    case none
    case urgent
    case high
    case medium
    case low

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .none: "No priority"
        case .urgent: "Urgent"
        case .high: "High"
        case .medium: "Medium"
        case .low: "Low"
        }
    }

    public static let displayOrder: [IssuePriority] = [.urgent, .high, .medium, .low, .none]

    public static func from(_ wire: String?) -> IssuePriority {
        guard let wire else { return .none }
        return IssuePriority(rawValue: wire) ?? .none
    }
}
