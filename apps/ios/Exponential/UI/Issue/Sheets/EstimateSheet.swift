import ExpCore
import ExpUI
import SwiftUI

/// The estimate picker (EXP-630): "No estimate" first, then the team scale's
/// ladder (plus an off-ladder current value, `estimatePickerValues`), each
/// labelled by `estimateLabel` on the team's scale ("XS"…"XL" or "1 point",
/// "5 points"); the current pick is checked. Mirrors the web's
/// `EstimateControl` picker; a pick commits at once and dismisses, like the
/// priority sheet it sits beside.
struct EstimateSheet: View {
    let current: Int?
    /// The team's scale (contract `issueEstimation`).
    let estimationType: String
    /// Nil = clear the estimate.
    let onSelect: (Int?) -> Void

    /// The "No estimate" row's value; every other row is its number.
    static let noneValue = "__none"

    static func pickerValue(_ value: Int?) -> String {
        value.map(String.init) ?? noneValue
    }

    var body: some View {
        let values: [Int?] = [nil] + estimatePickerValues(current: current, scale: estimationType)
        // EXP-1021: the shared picker's own sheet body — plain text rows on
        // every client (web `EstimateControl`, Android `EstimatePickerSheet`,
        // desktop menu): the value IS the label; the pick wears the check.
        GlassPickerContent(
            items: values.map { value in
                PickerItem(value: Self.pickerValue(value), label: estimateLabel(value, scale: estimationType))
            },
            mode: .single,
            value: [Self.pickerValue(current)],
            onChange: { picked in
                guard let id = picked.first else { return }
                onSelect(id == Self.noneValue ? nil : Int(id))
            },
            title: "Estimate"
        )
    }
}
