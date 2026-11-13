import ExpCore
import ExpPush
import ExpUI
import SwiftUI
import UIKit

/// #EXP-1638 (app-shortcuts.json): the app icon's long-press menu. The three
/// statics live in Info.plist (`UIApplicationShortcutItems`, Project.swift);
/// this publishes the recent chat as the dynamic item and routes a pick into
/// `DeepLinkBus`.
@MainActor
enum AppShortcutItems {
    /// Set once by `AppDependencies`; a pick that lands before it is kept.
    weak static var deps: AppDependencies? {
        didSet { if let pendingType { self.pendingType = nil; _ = handle(pendingType) } }
    }
    private static var pendingType: String?

    /// The dynamic items: every entry past the Info.plist statics (the OS
    /// lists the static ones first, so the menu reads as the fixture's list).
    static func publish(_ entries: [AppShortcuts.Entry]) {
        let items = entries.filter { !AppShortcuts.statics.contains($0.id) }.map { entry in
            UIApplicationShortcutItem(
                type: entry.id,
                localizedTitle: entry.title,
                localizedSubtitle: nil,
                icon: icon(entry.icon),
                userInfo: nil
            )
        }
        let app = UIApplication.shared
        let current = (app.shortcutItems ?? []).map { "\($0.type)\u{1}\($0.localizedTitle)" }
        guard current != items.map({ "\($0.type)\u{1}\($0.localizedTitle)" }) else { return }
        app.shortcutItems = items
    }

    /// Routes a pick; signed out or an unknown id = the normal start screen.
    @discardableResult
    static func handle(_ type: String) -> Bool {
        guard let deps else {
            pendingType = type
            return true
        }
        guard deps.auth.hasAuthenticatedAccount, let target = AppShortcuts.parseAppShortcut(type) else {
            return false
        }
        deps.deepLinkBus.navigateToShortcut(target)
        return true
    }

    private static func icon(_ concept: String) -> UIApplicationShortcutIcon? {
        guard let asset = AppIcons.concepts[concept].flatMap(AppIcons.assetName) else { return nil }
        return UIApplicationShortcutIcon(templateImageName: asset)
    }
}

/// The SwiftUI app's UIKit hooks: a scene delegate that receives the
/// shortcut on a cold launch (connection options) and a warm one.
final class ExponentialAppDelegate: NSObject, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(name: nil, sessionRole: connectingSceneSession.role)
        configuration.delegateClass = ExponentialSceneDelegate.self
        return configuration
    }

    /// #EXP-1815 (notification-delivery.json CLEARING): a silent push. A
    /// dismiss (rows read on any device) removes the delivered notifications
    /// it matches; it never shows anything.
    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any],
        fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
    ) {
        let payload = NotificationDismiss.payload(userInfo)
        guard payload["type"] == NotificationDelivery.DataKeys.dismissType else {
            completionHandler(.noData)
            return
        }
        nonisolated(unsafe) let done = completionHandler
        Task {
            let removed = await NotificationDismiss.removeDelivered(matching: payload)
            done(removed ? .newData : .noData)
        }
    }
}

final class ExponentialSceneDelegate: NSObject, UIWindowSceneDelegate {
    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        if let item = connectionOptions.shortcutItem {
            AppShortcutItems.handle(item.type)
        }
    }

    func windowScene(
        _ windowScene: UIWindowScene,
        performActionFor shortcutItem: UIApplicationShortcutItem,
        completionHandler: @escaping (Bool) -> Void
    ) {
        completionHandler(AppShortcutItems.handle(shortcutItem.type))
    }
}
