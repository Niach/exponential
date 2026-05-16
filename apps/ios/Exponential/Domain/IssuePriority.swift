import SwiftUI

enum IssuePriority: String, CaseIterable, Codable, Identifiable, Sendable {
    case none
    case urgent
    case high
    case medium
    case low

    var id: String { rawValue }

    var label: String {
        switch self {
        case .none: "No priority"
        case .urgent: "Urgent"
        case .high: "High"
        case .medium: "Medium"
        case .low: "Low"
        }
    }

    var sfSymbol: String {
        switch self {
        case .none: "minus"
        case .urgent: "exclamationmark.triangle.fill"
        case .high: "chevron.up"
        case .medium: "equal"
        case .low: "chevron.down"
        }
    }

    var color: Color {
        switch self {
        case .none: PriorityColor.none
        case .urgent: PriorityColor.urgent
        case .high: PriorityColor.high
        case .medium: PriorityColor.medium
        case .low: PriorityColor.low
        }
    }

    static let displayOrder: [IssuePriority] = [.urgent, .high, .medium, .low, .none]

    static func from(_ wire: String?) -> IssuePriority {
        guard let wire else { return .none }
        return IssuePriority(rawValue: wire) ?? .none
    }
}
