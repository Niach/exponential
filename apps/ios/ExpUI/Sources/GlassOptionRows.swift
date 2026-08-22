import SwiftUI

// The iOS twin of Android's ui/components/SheetOptionRows.kt `PickerRow`
// (EXP-603/EXP-607): label left, selected value + chevron right; a tap opens a
// `GlassPickerSheet` of the options. Both platforms retired their anchored
// dropdown here for the same reason — a dropdown over a sheet lands wherever
// the system can fit it, while a sheet always presents the same way and reads
// like every other picker in the app.
//
// Absent twins, deliberately: `SectionLabel`, `OptionGroup`, `GroupDivider`
// and `SwitchRow` are Compose reconstructions of what `Form`/`Section`/
// `Toggle` already give us here — Android had to hand-roll the inset-grouped
// look these sheets copied FROM iOS.

/// The fill a `Form` row wears once `.scrollContentBackground(.hidden)` has
/// taken the system grouped-list gray away (EXP-603) — the same white .05 the
/// glass rows elsewhere sit on, applied per `Section` via `.listRowBackground`.
public let glassFormRowFill = Color.white.opacity(0.05)

/// One `Form` row that picks a value from a list. `label` renders both the
/// trailing summary and each sheet row, so a call site names its vocabulary
/// once. Disabled dims the whole row and drops the tap.
public struct GlassPickerRow<SelectionValue: Hashable>: View {
    let title: String
    @Binding var selection: SelectionValue
    let options: [SelectionValue]
    let label: (SelectionValue) -> String
    var enabled: Bool = true

    @State private var showsOptions = false

    public init(
        _ title: String,
        selection: Binding<SelectionValue>,
        options: [SelectionValue],
        label: @escaping (SelectionValue) -> String,
        enabled: Bool = true
    ) {
        self.title = title
        self._selection = selection
        self.options = options
        self.label = label
        self.enabled = enabled
    }

    public var body: some View {
        HStack(spacing: 8) {
            Text(title)
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.primary : TextOpacity.quaternary))
            Spacer(minLength: 8)
            Text(label(selection))
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.secondary : TextOpacity.quaternary))
                .lineLimit(1)
            AppIcon(AppIcons.uiChevronRight, size: 14)
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.tertiary : TextOpacity.quaternary))
        }
        .contentShape(Rectangle())
        .onTapGesture {
            guard enabled else { return }
            showsOptions = true
        }
        .sheet(isPresented: $showsOptions) {
            GlassPickerSheet(
                title: title,
                items: options,
                selectedID: selection,
                idFor: { $0 },
                onSelect: { selection = $0 }
            ) { option in
                Text(label(option))
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
        }
    }
}
