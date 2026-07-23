import ExpUI
import ExpCore
import SwiftUI

/// Due-date sheet (EXP-240): graphical calendar (config lifted from
/// DueDatePicker), Start/End time rows (enabled once a date is set — same
/// server semantics: dueTime depends on dueDate), and a destructive
/// "Clear due date" row. Date/time edits commit immediately; the sheet stays
/// open for follow-up tweaks.
struct DueDateSheet: View {
    let date: Date?
    let dueTime: String?
    let endTime: String?
    let onDateChange: (Date?) -> Void
    let onDueTimeChange: (String?) -> Void
    let onEndTimeChange: (String?) -> Void

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
                    .tint(.blue)
                    .colorScheme(.dark)
                    // No date yet: the calendar previews today until a tap
                    // commits — dim it slightly so "unset" stays legible.
                    .opacity(date == nil ? 0.75 : 1)

                    VStack(spacing: 0) {
                        timeRow(label: "Start time", value: dueTime, onChange: onDueTimeChange)
                        Divider().background(Color.white.opacity(0.06))
                        timeRow(label: "End time", value: endTime, onChange: onEndTimeChange)
                    }
                    .padding(.vertical, 4)
                    .glassSection()
                    .disabled(date == nil)
                    .opacity(date == nil ? 0.45 : 1)

                    if date != nil {
                        Button {
                            onDateChange(nil)
                            dismiss()
                        } label: {
                            HStack(spacing: 8) {
                                Image(systemName: "xmark.circle")
                                    .font(.body)
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

    private func timeRow(label: String, value: String?, onChange: @escaping (String?) -> Void) -> some View {
        HStack {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
            TimeFieldButton(value: value, placeholder: "—", onChange: onChange)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
}
