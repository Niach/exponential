import ExpUI
import SwiftUI

/// Shared inline due-date picker used in both Create and Detail views.
/// Shows a row with the current date (or "None"), tappable to expand a graphical calendar.
struct DueDatePicker: View {
    @Binding var date: Date?
    /// When true, renders WITHOUT its own `.glassSection()` so the picker can sit
    /// as the last row inside another card (EXP-167 — the Status/Priority card).
    var embedded: Bool = false
    @State private var expanded = false

    var body: some View {
        Group {
            if embedded {
                core
            } else {
                core.glassSection()
            }
        }
        .animation(.easeInOut(duration: 0.2), value: expanded)
    }

    private var core: some View {
        VStack(spacing: 0) {
            Button {
                expanded.toggle()
                if expanded && date == nil {
                    date = Date()
                }
            } label: {
                HStack {
                    AppIcon(AppIcons.uiDueDate, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 22)
                    Text("Due date")
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    Spacer()
                    if let d = date {
                        Text(formatted(d))
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Button {
                            date = nil
                            expanded = false
                        } label: {
                            AppIcon(AppIcons.uiClear, size: AppIcon.Size.medium)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        .buttonStyle(.plain)
                    } else {
                        Text("None")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    AppIcon(expanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
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
