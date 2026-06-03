import ExpCore
import ExpUI
import SwiftUI

// Shared issue-property controls for the macOS create + detail surfaces.
// Mirrors the iOS TimeFieldButton / RecurrencePickerSheet / formatRecurrence,
// substituting macOS-native chrome (Menu + Popover) for the iOS bottom sheets
// and wheel picker (`.wheel` doesn't exist on macOS).

/// "HH:mm" wire format for dueTime/endTime. The iOS app uses the shared
/// `AppDateFormatters`, which lives in the iOS target only — the Mac target
/// keeps its own equivalent.
let macHHmmFormatter: DateFormatter = {
    let f = DateFormatter()
    f.dateFormat = "HH:mm"
    f.locale = Locale(identifier: "en_US_POSIX")
    return f
}()

/// Same wording as the iOS `formatRecurrence` (Daily/Weekly/Monthly for
/// interval == 1, else "Every N units").
func formatRecurrence(_ interval: Int?, _ unit: String?) -> String {
    guard let interval, let unitRaw = unit, let unit = RecurrenceUnit(rawValue: unitRaw) else {
        return "Doesn't repeat"
    }
    if interval == 1 {
        switch unit {
        case .day: return "Daily"
        case .week: return "Weekly"
        case .month: return "Monthly"
        }
    }
    return "Every \(interval) \(unit.label(for: interval))"
}

/// Short relative time ("2h ago") from an ISO-8601 timestamp. Ports the iOS
/// CommentThreadView helper.
func macRelativeDate(_ s: String) -> String {
    let iso = ISO8601DateFormatter()
    iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let date = iso.date(from: s) ?? ISO8601DateFormatter().date(from: s)
    guard let date else { return "" }
    let f = RelativeDateTimeFormatter()
    f.unitsStyle = .short
    return f.localizedString(for: date, relativeTo: Date())
}

/// Nullable "HH:mm" time field — a popover with an hour/minute picker and a
/// Clear affordance. The iOS sibling uses `.datePickerStyle(.wheel)`, which is
/// unavailable on macOS, so this uses the default stepper field.
struct MacTimeFieldButton: View {
    let value: String?
    var placeholder: String = "—"
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
                .foregroundStyle(value == nil ? Color.secondary : Color.primary)
        }
        .buttonStyle(.borderless)
        .fixedSize()
        .popover(isPresented: $showPicker, arrowEdge: .bottom) {
            VStack(spacing: 12) {
                DatePicker("Time", selection: $draft, displayedComponents: [.hourAndMinute])
                    .datePickerStyle(.stepperField)
                    .labelsHidden()
                HStack {
                    if value != nil {
                        Button("Clear", role: .destructive) { onChange(nil); showPicker = false }
                    }
                    Spacer()
                    Button("Cancel") { showPicker = false }
                    Button("Save") { onChange(macHHmmFormatter.string(from: draft)); showPicker = false }
                        .buttonStyle(.borderedProminent)
                        .tint(Accent.indigo)
                }
            }
            .padding(16)
            .frame(width: 240)
        }
    }

    private func parseTime(_ value: String?) -> Date? {
        guard let value else { return nil }
        return macHHmmFormatter.date(from: value)
    }

    private func defaultTime() -> Date {
        var c = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        c.hour = 9
        c.minute = 0
        return Calendar.current.date(from: c) ?? Date()
    }
}

/// Wrapping flow layout for label chips (ports the iOS `FlowLayout`, which
/// lives in the iOS target).
struct MacFlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(proposal: proposal, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrange(proposal: proposal, subviews: subviews)
        for (index, position) in result.positions.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y), proposal: .unspecified)
        }
    }

    private func arrange(proposal: ProposedViewSize, subviews: Subviews) -> (positions: [CGPoint], size: CGSize) {
        let maxWidth = proposal.width ?? .infinity
        var positions: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var maxX: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > maxWidth && x > 0 {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            positions.append(CGPoint(x: x, y: y))
            rowHeight = max(rowHeight, size.height)
            x += size.width + spacing
            maxX = max(maxX, x)
        }
        return (positions, CGSize(width: maxX, height: y + rowHeight))
    }
}

/// Recurrence picker as a macOS `Menu` with one submenu per `RecurrenceUnit`
/// (interval and unit always commit together, matching the server's
/// assertRecurrencePair). "Doesn't repeat" clears both.
struct MacRecurrenceMenu: View {
    let interval: Int?
    let unit: String?
    let onSelect: (Int?, RecurrenceUnit?) -> Void
    var enabled: Bool = true

    var body: some View {
        Menu {
            Button("Doesn't repeat") { onSelect(nil, nil) }
            ForEach(RecurrenceUnit.allCases) { u in
                Menu(u.label(for: 2).capitalized) {
                    ForEach(DomainContract.recurrenceIntervals, id: \.self) { n in
                        Button("Every \(n) \(u.label(for: n))") { onSelect(n, u) }
                    }
                }
            }
        } label: {
            Label(formatRecurrence(interval, unit), systemImage: "repeat")
                .foregroundStyle(interval == nil ? Color.secondary : Color.primary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .disabled(!enabled)
    }
}
