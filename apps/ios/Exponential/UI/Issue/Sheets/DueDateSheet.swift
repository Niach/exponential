import ExpUI
import ExpCore
import SwiftUI

/// Due-date sheet (EXP-240): graphical calendar (config lifted from
/// DueDatePicker) plus a destructive "Clear due date" row. Due date is
/// date-only — there is no time of day. Edits commit immediately; the sheet
/// stays open for follow-up tweaks.
struct DueDateSheet: View {
    let date: Date?
    let onDateChange: (Date?) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        GlassSheetChrome(title: "Due date", detents: [.large]) {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    DatePicker(
                        "",
                        selection: Binding(
                            get: { date ?? Date() },
                            set: { onDateChange($0) }
                        ),
                        displayedComponents: .date
                    )
                    .datePickerStyle(.graphical)
                    .labelsHidden()
                    .tint(DesignTokens.Palette.primary)
                    .colorScheme(.dark)
                    // No date yet: the calendar previews today until a tap
                    // commits — dim it slightly so "unset" stays legible.
                    .opacity(date == nil ? 0.75 : 1)

                    if date != nil {
                        Button {
                            onDateChange(nil)
                            dismiss()
                        } label: {
                            HStack(spacing: 8) {
                                AppIcon(AppIcons.uiClear, size: AppIcon.Size.medium)
                                Text("Clear due date")
                                    .font(.subheadline)
                                Spacer(minLength: 0)
                            }
                            .foregroundStyle(DesignTokens.Semantic.red)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                            .glassSection()
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 24)
            }
        }
    }
}
