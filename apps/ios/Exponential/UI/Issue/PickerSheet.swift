import SwiftUI

/// A reusable bottom-sheet picker used by the issue editor surfaces in place
/// of `Menu` popovers. Matches Linear's mobile UX where property pickers
/// open as half-height sheets containing a tap-target-friendly list of
/// options.
///
/// Selecting a row commits immediately via `onSelect` and dismisses the
/// sheet. The current selection is shown with a trailing checkmark.
struct PickerSheet<Item, ID: Hashable, Row: View>: View {
    let title: String
    let items: [Item]
    let selectedID: ID?
    let idFor: (Item) -> ID
    let onSelect: (Item) -> Void
    @ViewBuilder let row: (Item) -> Row

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                ForEach(items, id: idFor) { item in
                    Button {
                        onSelect(item)
                        dismiss()
                    } label: {
                        HStack {
                            row(item)
                            Spacer()
                            if let selectedID, idFor(item) == selectedID {
                                Image(systemName: "checkmark")
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(.tint)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

// MARK: - Assignee picker helper

/// Row model for the assignee `PickerSheet`. Wraps an optional user so we
/// can render the "Unassigned" sentinel as a first-class option with a
/// stable identifier (`"__unassigned"`).
struct AssigneeOption: Identifiable, Hashable {
    let id: String
    let userId: String?
    let displayName: String

    static let unassigned = AssigneeOption(
        id: "__unassigned",
        userId: nil,
        displayName: "Unassigned"
    )
}

func assigneeOptions(users: [UserEntity]) -> [AssigneeOption] {
    var options: [AssigneeOption] = [.unassigned]
    options.append(contentsOf: users.map { user in
        AssigneeOption(
            id: user.id,
            userId: user.id,
            displayName: user.name ?? user.email
        )
    })
    return options
}

// MARK: - Recurrence picker sheet

/// Recurrence has a sectioned layout (one section per `RecurrenceUnit`)
/// that doesn't map cleanly onto the flat `PickerSheet`, so it gets its
/// own inline implementation. Matches the same look-and-feel: medium
/// detent, list with a checkmark on the current selection, immediate
/// commit on tap.
struct RecurrencePickerSheet: View {
    let currentInterval: Int?
    let currentUnit: String?
    let onSelect: (Int?, RecurrenceUnit?) -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Button {
                        onSelect(nil, nil)
                        dismiss()
                    } label: {
                        HStack {
                            Label("Doesn't repeat", systemImage: "xmark")
                            Spacer()
                            if currentInterval == nil {
                                Image(systemName: "checkmark")
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(.tint)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }

                ForEach(RecurrenceUnit.allCases) { unit in
                    Section(unit.label(for: 2).capitalized) {
                        ForEach(recurrenceIntervals, id: \.self) { interval in
                            Button {
                                onSelect(interval, unit)
                                dismiss()
                            } label: {
                                HStack {
                                    Text("Every \(interval) \(unit.label(for: interval))")
                                    Spacer()
                                    if currentInterval == interval && currentUnit == unit.rawValue {
                                        Image(systemName: "checkmark")
                                            .font(.subheadline.weight(.semibold))
                                            .foregroundStyle(.tint)
                                    }
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
            .navigationTitle("Repeat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
