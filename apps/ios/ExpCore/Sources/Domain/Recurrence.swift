import Foundation

public enum RecurrenceUnit: String, CaseIterable, Identifiable, Sendable {
    case day
    case week
    case month

    public var id: String { rawValue }

    public func label(for interval: Int) -> String {
        switch self {
        case .day: interval == 1 ? "day" : "days"
        case .week: interval == 1 ? "week" : "weeks"
        case .month: interval == 1 ? "month" : "months"
        }
    }
}
