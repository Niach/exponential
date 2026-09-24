import Foundation

// EXP-630: story points, rendered on the TEAM's scale (`teams.estimation_type`,
// contract `issueEstimation`). The stored value is always a point number, so a
// team can switch scales without touching a single issue; `none` hides the
// chip everywhere. T-shirt sizes are the fibonacci points worn as XS…XL,
// exactly how Linear stores them. Mirrors web `lib/issue-estimate.ts`, locked
// ×4 against `domain-contract/fixtures/issue-estimate.json`.
public enum IssueEstimate {
    /// The ladders per scale (`none` has none). Keyed by the contract value.
    public static let scales: [String: [Int]] = [
        DomainContract.issueEstimationExponential: [1, 2, 4, 8, 16],
        DomainContract.issueEstimationFibonacci: [1, 2, 3, 5, 8],
        DomainContract.issueEstimationLinear: [1, 2, 3, 4, 5],
        DomainContract.issueEstimationTshirt: [1, 2, 3, 5, 8],
    ]

    /// The t-shirt sizes, index-aligned with the `tshirt` ladder.
    public static let tshirtLabels: [String] = ["XS", "S", "M", "L", "XL"]

    public static let noEstimate = "No estimate"

    /// The ladder of one scale; empty for `none` and any unknown scale.
    public static func scale(_ type: String) -> [Int] {
        scales[type] ?? []
    }

    /// True when the team's scale renders the estimate control at all.
    public static func isEnabled(_ type: String?) -> Bool {
        guard let type else { return false }
        return scales[type] != nil
    }

    private static func tshirtLabel(_ value: Int) -> String? {
        guard let index = scales[DomainContract.issueEstimationTshirt]?.firstIndex(of: value),
              index < tshirtLabels.count
        else { return nil }
        return tshirtLabels[index]
    }

    /// "No estimate", "M", "1 point", "5 points".
    public static func label(_ value: Int?, scale type: String = DomainContract.issueEstimationFibonacci) -> String {
        guard let value else { return noEstimate }
        if type == DomainContract.issueEstimationTshirt, let size = tshirtLabel(value) {
            return size
        }
        return value == 1 ? "1 point" : "\(value) points"
    }

    /// The chip form: "M" on the t-shirt scale, "5 pt" elsewhere.
    public static func shortLabel(_ value: Int, scale type: String = DomainContract.issueEstimationFibonacci) -> String {
        if type == DomainContract.issueEstimationTshirt, let size = tshirtLabel(value) {
            return size
        }
        return "\(value) pt"
    }

    /// The values a picker offers: the scale's ladder plus the current value
    /// when it sits off the ladder (an import from another scale, a value set
    /// before the scale was switched), ascending — so the trigger always names
    /// a listed option. The "No estimate" row is the caller's.
    public static func pickerValues(current: Int?, scale type: String) -> [Int] {
        var values = Set(scale(type))
        if let current, current >= 0 { values.insert(current) }
        return values.sorted()
    }

    /// The `estimate_changed` timeline phrase, on the team's scale. The
    /// payload's `to` may arrive as a number OR a numeric string (the event
    /// row is stringified JSON on every client); anything else reads as
    /// cleared.
    public static func eventPhrase(payload: [String: Any]?, scale type: String = DomainContract.issueEstimationFibonacci) -> String {
        guard let to = payloadInt(payload?["to"]) else { return "removed the estimate" }
        return "set the estimate to \(label(to, scale: type))"
    }

    private static func payloadInt(_ raw: Any?) -> Int? {
        switch raw {
        case let value as Int:
            return value
        case let value as Double:
            return value.isFinite ? Int(value) : nil
        case let value as String:
            guard !value.isEmpty, let parsed = Double(value), parsed.isFinite else { return nil }
            return Int(parsed)
        default:
            return nil
        }
    }
}

// The web's free-function names, for call sites that read like the reference.
public func estimateLabel(_ value: Int?, scale: String = DomainContract.issueEstimationFibonacci) -> String {
    IssueEstimate.label(value, scale: scale)
}

public func estimateShortLabel(_ value: Int, scale: String = DomainContract.issueEstimationFibonacci) -> String {
    IssueEstimate.shortLabel(value, scale: scale)
}

public func estimatePickerValues(current: Int?, scale: String) -> [Int] {
    IssueEstimate.pickerValues(current: current, scale: scale)
}

public func estimateEventPhrase(payload: [String: Any]?, scale: String = DomainContract.issueEstimationFibonacci) -> String {
    IssueEstimate.eventPhrase(payload: payload, scale: scale)
}
