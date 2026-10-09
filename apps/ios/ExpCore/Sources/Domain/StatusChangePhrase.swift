import Foundation

/// The `status_changed` activity phrase ×4 (polish round, pinned):
/// `changed status from {from} to {to}`, naming the team status rows as the
/// app shows them ("In Progress"). Mirrors web `lib/issue-event-labels.ts`
/// `statusLabel` / `issueEventPhrase`: the payload's name snapshot
/// (`fromName`/`toName`, EXP-314) wins, a name-less legacy row reads the
/// builtin row's display name, a retired token keeps its historic label
/// (EXP-685 `todo`), anything else the underscore munge.
public enum StatusChangePhrase {
    /// Retired enum tokens old events still name.
    public static let retiredLabels: [String: String] = ["todo": "Todo"]

    /// One side's display name; nil when the side is absent.
    public static func label(name: String?, anchor: String?) -> String? {
        if let name, !name.isEmpty { return name }
        guard let anchor, !anchor.isEmpty else { return nil }
        if let retired = retiredLabels[anchor] { return retired }
        if let index = DomainContract.issueStatusDefaultKeys.firstIndex(of: anchor),
           index < DomainContract.issueStatusDefaultNames.count {
            return DomainContract.issueStatusDefaultNames[index]
        }
        return anchor.replacingOccurrences(of: "_", with: " ")
    }

    /// The whole phrase off a payload's four fields.
    public static func phrase(
        fromName: String?, fromAnchor: String?, toName: String?, toAnchor: String?
    ) -> String {
        guard let to = label(name: toName, anchor: toAnchor) else { return "changed status" }
        if let from = label(name: fromName, anchor: fromAnchor) {
            return "changed status from \(from) to \(to)"
        }
        return "changed status to \(to)"
    }
}
