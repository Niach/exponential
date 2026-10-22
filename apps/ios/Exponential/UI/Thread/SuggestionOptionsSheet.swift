import ExpCore
import ExpUI
import SwiftUI

/// EXP-1323: a suggested task's Options on a phone — THE Menu in its SHEET
/// presentation (web `ThreadSuggestionsCard`'s Menu), titled with the item:
/// Do it here · Start on… (the runnable devices) · Start with… (contract `codingModel`,
/// never a default row) · File as issue · Dismiss. A submenu swaps the
/// sheet's rows in place; a pick hands the action back and the host runs it
/// once the sheet has dismissed (the `ThreadMenuSheet` rule).
struct SuggestionOptionsSheet: View {
    enum Action: Equatable {
        case here
        case startOn(deviceId: String)
        case startWith(model: String)
        case file
        case dismiss
    }

    struct Device: Identifiable, Equatable {
        let id: String
        let name: String
        let icon: String
    }

    private enum Level {
        case root, devices, models
    }

    let title: String
    /// The runnable devices; nil while they load.
    let devices: [Device]?
    let onPick: (Action) -> Void

    @State private var level = Level.root

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                switch level {
                case .root:
                    row(Copy.here, icon: AppIcons.uiSend, id: "here") { onPick(.here) }
                    submenuRow(Copy.startOn, icon: AppIcons.uiDevice, enabled: !(devices ?? []).isEmpty, id: "start-on") {
                        level = .devices
                    }
                    submenuRow(Copy.startWith, icon: AppIcons.uiProperties, enabled: true, id: "start-with") {
                        level = .models
                    }
                    row(Copy.file, icon: AppIcons.uiIssue, id: "file") { onPick(.file) }
                    row(Copy.dismiss, icon: AppIcons.uiClose, id: "dismiss") { onPick(.dismiss) }
                case .devices:
                    backRow(Copy.startOn)
                    ForEach(devices ?? []) { device in
                        row(device.name, icon: device.icon, id: "device-\(device.id)") {
                            onPick(.startOn(deviceId: device.id))
                        }
                    }
                case .models:
                    backRow(Copy.startWith)
                    ForEach(DomainContract.codingModelValues, id: \.self) { model in
                        row(LaunchVocabulary.modelLabel(model), icon: nil, id: "model-\(model)") {
                            onPick(.startWith(model: model))
                        }
                    }
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("thread-suggestion-options-sheet")
        }
    }

    private func row(_ title: String, icon: String?, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            GlassMenuItemLabel(title, icon: icon)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread-suggestion-menu-\(id)")
    }

    private func submenuRow(
        _ title: String, icon: String, enabled: Bool, id: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 0) {
                GlassMenuItemLabel(title, icon: icon, enabled: enabled)
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(.white.opacity(enabled ? TextOpacity.tertiary : TextOpacity.quaternary))
                    .padding(.trailing, GlassMenuTokens.itemHPadding)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityIdentifier("thread-suggestion-menu-\(id)")
    }

    /// A submenu's first row: back to the root rows.
    private func backRow(_ title: String) -> some View {
        Button {
            level = .root
        } label: {
            GlassMenuItemLabel(title, icon: AppIcons.uiChevronLeft)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread-suggestion-menu-back")
    }
}
