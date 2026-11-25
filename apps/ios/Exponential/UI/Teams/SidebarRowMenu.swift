import ExpCore
import ExpUI
import SwiftUI
import UIKit

/// #EXP-2126 (sidebar-menus.json, every navigation row): a row's long-press
/// menu, THE sheet the Teams tab's rows open (`TeamMenuSheet`). No action =
/// the tap alone. A pick runs once the sheet has dismissed.
private struct SidebarRowMenu: ViewModifier {
    let kind: SidebarMenus.Kind
    let title: String
    let actions: [SidebarMenus.Action]
    let onTap: (() -> Void)?
    let onPick: (SidebarMenus.Action) -> Void

    @State private var presented = false
    @State private var pending: SidebarMenus.Action?

    private var menu: some View {
        TeamMenuSheet(
            title: title,
            items: actions.enumerated().map { index, action in
                .init(
                    pick: "\(index)-\(action.rawValue)", label: SidebarMenus.copy[action] ?? "",
                    icon: SidebarMenus.icons[action].flatMap { AppIcons.conceptOrName($0) },
                    testId: "sidebar-menu-\(action.rawValue)",
                    destructive: SidebarMenus.destructive(kind, action), separator: action == .separator
                )
            }
        ) { (pick: String) in
            pending = actions.indices.first { pick.hasPrefix("\($0)-") }.map { actions[$0] }
            presented = false
        }
    }

    func body(content: Content) -> some View {
        Group {
            if actions.isEmpty {
                content.glassLongPress(enabled: false, onTap: onTap, onHold: {})
            } else {
                content
                    .glassLongPress(onTap: onTap, onHold: { presented = true })
                    .accessibilityAction(named: "Actions") { presented = true }
            }
        }
        .background {
            Color.clear.sheet(isPresented: $presented, onDismiss: {
                guard let action = pending else { return }
                pending = nil
                onPick(action)
            }) { menu }
        }
    }
}

extension View {
    func sidebarRowMenu(
        _ kind: SidebarMenus.Kind,
        title: String,
        actions: [SidebarMenus.Action],
        onTap: (() -> Void)? = nil,
        onPick: @escaping (SidebarMenus.Action) -> Void
    ) -> some View {
        modifier(SidebarRowMenu(kind: kind, title: title, actions: actions, onTap: onTap, onPick: onPick))
    }
}
