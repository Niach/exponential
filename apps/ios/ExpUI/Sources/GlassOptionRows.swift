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
/// taken the system grouped-list gray away (EXP-603) — the same `fillRow` the
/// glass rows elsewhere sit on, applied per `Section` via `.listRowBackground`.
public let glassFormRowFill = GlassTokens.fillRow

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

/// `GlassPickerRow`'s twin for a time of day (EXP-721): the same row geometry,
/// a wheel in a `GlassSheetChrome` instead of an option list. It replaces the
/// stock compact `DatePicker`, whose grey pill was the last system control
/// left among the glass rows — and whose 12/24-hour rendering follows the
/// PHONE's locale, so the same automation read "9:00 AM" here and "09:00" on
/// Android, desktop and web.
public struct GlassTimeRow: View {
    let title: String
    @Binding var selection: Date
    var enabled: Bool = true

    @State private var showsPicker = false

    public init(_ title: String, selection: Binding<Date>, enabled: Bool = true) {
        self.title = title
        self._selection = selection
        self.enabled = enabled
    }

    /// 24-hour `HH:mm` in the phone's own calendar — the cross-client wire
    /// LOOK, pinned by `GlassTimeRowTests`. A fixed `en_US_POSIX` locale keeps
    /// the format literal: a locale-driven one would re-introduce AM/PM.
    public static func formatted(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar.current
        formatter.timeZone = Calendar.current.timeZone
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: date)
    }

    public var body: some View {
        HStack(spacing: 8) {
            Text(title)
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.primary : TextOpacity.quaternary))
            Spacer(minLength: 8)
            Text(Self.formatted(selection))
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.secondary : TextOpacity.quaternary))
                .lineLimit(1)
            AppIcon(AppIcons.uiChevronRight, size: 14)
                .foregroundStyle(.white.opacity(enabled ? TextOpacity.tertiary : TextOpacity.quaternary))
        }
        .contentShape(Rectangle())
        .onTapGesture {
            guard enabled else { return }
            showsPicker = true
        }
        .sheet(isPresented: $showsPicker) {
            GlassTimeSheet(title: title, initial: selection) { selection = $0 }
        }
    }
}

/// The wheel behind `GlassTimeRow`, in the ONE sheet shell. The edit is
/// LOCAL until "Done" — a wheel commits on every detent otherwise, and a swipe
/// away would leave the half-scrolled value behind.
private struct GlassTimeSheet: View {
    let title: String
    let initial: Date
    let onCommit: (Date) -> Void

    @State private var local: Date
    @Environment(\.dismiss) private var dismiss

    init(title: String, initial: Date, onCommit: @escaping (Date) -> Void) {
        self.title = title
        self.initial = initial
        self.onCommit = onCommit
        _local = State(initialValue: initial)
    }

    var body: some View {
        GlassSheetChrome(
            title: title,
            content: {
                DatePicker("", selection: $local, displayedComponents: .hourAndMinute)
                    .datePickerStyle(.wheel)
                    .labelsHidden()
                    .colorScheme(.dark)
                    .tint(DesignTokens.Palette.primary)
                    .padding(.horizontal, 6)
                    .padding(.bottom, 8)
            },
            primaryAction: {
                GlassSubmitButton("Done") {
                    onCommit(local)
                    dismiss()
                }
            }
        )
    }
}
