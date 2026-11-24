import ExpCore
import ExpUI
import SwiftUI

/// The glyph colour of an app with none: the row's muted ink.
enum AppGlyph {
    static let neutralColor = "#a1a1aa"
}

/// THE version pill (`copy.version`): `v{n}`, or the `Draft` badge muted.
struct AppVersionPill: View {
    let app: AppListEntry

    var body: some View {
        GlassPill(app.pillLabel)
            .opacity(app.pillMuted ? 0.6 : 1)
            .monospacedDigit()
            .accessibilityIdentifier("app-version-pill")
    }
}

/// An app row's `…` — THE Menu in its sheet presentation: Export (VAPP-121:
/// any member, once the app has a version) · Delete app (destructive; the
/// creator or an owner). No Rename (VAPP-176: the agent names an app). A pick hands the action back; the
/// host acts once the sheet has dismissed. VAPP-177: the phone editor's `…`
/// (`versionsCount` set) leads with Versions (the count as trailing meta)
/// and Settings (`editorTabs.menu`); eApps V1 (#VAPP-210): Publish comes
/// FIRST while `Apps.canPublish` and the viewer may write the app.
struct AppMenuSheet: View {
    enum Action { case edit, publish, versions, settings, export, delete }

    let title: String
    /// VAPP-242 (`run.menu`): Edit app FIRST (the list row's and the run
    /// view's menus; the creator or an owner) = THE EDITOR.
    var canEdit = false
    /// eApps V1: the editor's Publish (the head is ahead of the published one).
    var canPublish = false
    var canExport = false
    var canManage = true
    /// nil = the row's menu (no Versions / Settings).
    var versionsCount: Int? = nil
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: title) {
            VStack(spacing: 0) {
                if canEdit {
                    Button { onPick(.edit) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.edit, icon: AppIcons.uiEdit)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-edit")
                }
                if canPublish {
                    Button { onPick(.publish) } label: {
                        GlassMenuItemLabel(Apps.Copy.publish, icon: AppIcons.uiSubmit)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(Apps.TestIds.publish)
                }
                if let versionsCount {
                    Button { onPick(.versions) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.versions, icon: AppIcons.uiHistory)
                            .overlay(alignment: .trailing) {
                                Text(String(versionsCount))
                                    .font(.system(size: DesignTokens.Typography.Size.sm).monospacedDigit())
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    .padding(.trailing, GlassMenuTokens.itemHPadding)
                            }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-versions")
                    Button { onPick(.settings) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.settings, icon: AppIcons.navSettings)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-settings")
                }
                if canExport {
                    Button { onPick(.export) } label: {
                        GlassMenuItemLabel(AppExport.Copy.export, icon: AppIcons.uiShare)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier(AppExport.TestIds.menuItem)
                }
                if canManage {
                    Button { onPick(.delete) } label: {
                        GlassMenuItemLabel(Apps.Copy.Menu.delete, icon: AppIcons.uiDelete, destructive: true)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("app-menu-delete")
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("app-menu-sheet")
        }
    }
}

/// VAPP-242 THE NEW MENU (`run.newMenu`, testId `newMenu`): the band's
/// `New app` pill opens THE Menu in its sheet presentation (the row's `…`
/// alike): New app (ui-add) · Import… (ui-file). A pick hands the action
/// back; the list acts once the sheet has dismissed.
struct AppsNewMenuSheet: View {
    enum Action { case newApp, importApp }

    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: Apps.Copy.newApp) {
            VStack(spacing: 0) {
                Button { onPick(.newApp) } label: {
                    GlassMenuItemLabel(Apps.Copy.newApp, icon: AppIcons.uiAdd)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("apps-new-menu-new")
                Button { onPick(.importApp) } label: {
                    GlassMenuItemLabel(Apps.Copy.import, icon: AppIcons.uiFile)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(Apps.TestIds.importApp)
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Apps.TestIds.newMenu)
        }
    }
}
