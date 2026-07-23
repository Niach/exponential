import ExpUI
import SwiftUI

// Inline time picker that surfaces the iOS wheel picker via a popover
// and a "Clear" affordance for nullable time values. Shared by the create
// sheet and the due-date sheet (moved out of IssueDetailView, EXP-240).
struct TimeFieldButton: View {
    let value: String?
    let placeholder: String
    let onChange: (String?) -> Void

    @State private var showPicker = false
    @State private var draft = Date()

    var body: some View {
        Button {
            draft = parseTime(value) ?? defaultTime()
            showPicker = true
        } label: {
            Text(value ?? placeholder)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(value == nil
                    ? .white.opacity(TextOpacity.tertiary)
                    : .white)
        }
        .popover(isPresented: $showPicker) {
            VStack(spacing: 12) {
                DatePicker("Time", selection: $draft, displayedComponents: [.hourAndMinute])
                    .datePickerStyle(.wheel)
                    .labelsHidden()
                HStack {
                    if value != nil {
                        Button("Clear") {
                            onChange(nil)
                            showPicker = false
                        }
                        .tint(.red)
                    }
                    Spacer()
                    Button("Cancel") { showPicker = false }
                    Button("Save") {
                        onChange(formatTime(draft))
                        showPicker = false
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .padding(16)
            .presentationCompactAdaptation(.popover)
        }
    }

    private func parseTime(_ value: String?) -> Date? {
        guard let value else { return nil }
        return AppDateFormatters.HHmm.date(from: value)
    }

    private func formatTime(_ date: Date) -> String {
        AppDateFormatters.HHmm.string(from: date)
    }

    private func defaultTime() -> Date {
        var components = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        components.hour = 9
        components.minute = 0
        return Calendar.current.date(from: components) ?? Date()
    }
}
