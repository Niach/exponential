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

    public var sfSymbol: String {
        switch self {
        case .none: "minus"
        case .urgent: "exclamationmark.triangle.fill"
        case .high: "chevron.up"
        case .medium: "equal"
        case .low: "chevron.down"
        }
    }

    public static let displayOrder: [IssuePriority] = [.urgent, .high, .medium, .low, .none]

    public static func from(_ wire: String?) -> IssuePriority {
        guard let wire else { return .none }
        return IssuePriority(rawValue: wire) ?? .none
    }
}
