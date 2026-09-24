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

    /// One picker row; `nil` is the "No estimate" sentinel.
    private struct Option: Identifiable {
        let value: Int?
        var id: String { value.map(String.init) ?? "__none" }
    }

    var body: some View {
        let options = [Option(value: nil)]
            + estimatePickerValues(current: current, scale: estimationType).map { Option(value: $0) }
        GlassPickerSheet(
            title: "Estimate",
            items: options,
            selectedID: Option(value: current).id,
            idFor: { $0.id },
            onSelect: { onSelect($0.value) }
        ) { option in
            Label {
                Text(estimateLabel(option.value, scale: estimationType))
            } icon: {
                AppIcon(
                    option.value == nil ? AppIcons.uiClear : AppIcons.uiEstimate,
                    size: AppIcon.Size.medium
                )
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }
}
