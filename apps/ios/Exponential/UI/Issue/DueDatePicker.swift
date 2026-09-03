import ExpUI
import SwiftUI

/// The inline due-date picker of the New-issue page: a row with the current
/// date (or "None"), tappable to unfold a graphical calendar. It carries NO
/// card of its own — it is the last row inside the property card (EXP-167), and
/// wears the shared `GlassMetaRowTokens` geometry so it reads as the fourth of
/// those rows. (EXP-698 r4 dropped the `embedded` flag: the standalone,
/// self-`glassSection()`ing variant had no call site left.)
struct DueDatePicker: View {
    @Binding var date: Date?
    @State private var expanded = false

    var body: some View {
        VStack(spacing: 0) {
            Button {
                expanded.toggle()
                if expanded && date == nil {
                    date = Date()
                }
            } label: {
                // EXP-698 r4: the same row as the Status/Priority/Assignee
                // rows above it — label left, glyph beside the value right, no
                // up/down chevron (the calendar unfolding IS the feedback).
                HStack(spacing: 6) {
                    Text("Due date")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Spacer(minLength: 8)
                    AppIcon(AppIcons.uiDueDate, size: GlassMetaRowTokens.glyphSize)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    if let d = date {
                        Text(formatted(d))
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        // The only way back to "no due date" — the row itself
                        // opens the calendar, so clearing needs its own target.
                        Button {
                            date = nil
                            expanded = false
                        } label: {
                            AppIcon(AppIcons.uiClear, size: GlassMetaRowTokens.glyphSize)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Clear due date")
                    } else {
                        Text("None")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .padding(.horizontal, GlassMetaRowTokens.horizontalPadding)
                .padding(.vertical, GlassMetaRowTokens.verticalPadding)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if expanded, date != nil {
                GlassDivider()
                DatePicker(
                    "",
                    selection: Binding(
                        get: { date ?? Date() },
                        set: { date = $0 }
                    ),
                    displayedComponents: .date
                )
                .datePickerStyle(.graphical)
                .labelsHidden()
                .tint(DesignTokens.Palette.primary)
                .colorScheme(.dark)
                .padding(.horizontal, 8)
                .padding(.bottom, 8)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: expanded)
    }

    private func formatted(_ date: Date) -> String {
        let cal = Calendar.current
        if cal.isDateInToday(date) { return "Today" }
        if cal.isDateInTomorrow(date) { return "Tomorrow" }
        let f = DateFormatter()
        f.dateFormat = "MMM d, yyyy"
        return f.string(from: date)
    }
}
