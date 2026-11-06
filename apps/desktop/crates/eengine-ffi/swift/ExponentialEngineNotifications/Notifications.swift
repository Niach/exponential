// #VAPP-265, #VAPP-268: `notifications`. Its own module so an iOS app links
// UserNotifications only when it declares the capability (ITMS-90683).
import Foundation
import ExponentialEngine
import ExponentialEngineCapabilityCore

public enum NotificationAuthorization: Equatable { case notDetermined, denied, granted }

/// When a notification fires.
public enum NotificationTrigger: Equatable {
    /// Deliver now (`at` null or not in the future).
    case now
    /// In `seconds` (`at` under a minute ahead).
    case after(TimeInterval)
    /// At these calendar components (`at` further ahead; year … second + the zone).
    case on(DateComponents)
}

/// One notification as the scheduler adds it.
public struct PlannedNotification: Equatable {
    /// The Engine's id (`<app>/<id>`) = the platform request's identifier.
    public var id: String
    public var title: String
    public var body: String?
    public var trigger: NotificationTrigger
    /// `NotificationTaps.userInfo`: the id, and the tap JSON when there is one.
    public var userInfo: [String: String]
    public init(id: String, title: String, body: String?, trigger: NotificationTrigger, userInfo: [String: String]) {
        self.id = id; self.title = title; self.body = body; self.trigger = trigger; self.userInfo = userInfo
    }
}

public protocol NotificationScheduler: AnyObject {
    func authorization() throws -> NotificationAuthorization
    /// Asks the person; true = allowed.
    func requestAuthorization() throws -> Bool
    /// Adds (or, for a known identifier, replaces) the request.
    func schedule(_ notification: PlannedNotification) throws
    func pendingIds() throws -> [String]
    func deliveredIds() throws -> [String]
    /// Withdraws pending AND delivered ones with these identifiers.
    func remove(ids: [String]) throws
}

/// When a notification for `at` fires, seen from `now`.
public enum NotificationPlan {
    public static func trigger(at: Date?, now: Date, calendar: Calendar = .current) -> NotificationTrigger {
        guard let at else { return .now }
        let seconds = at.timeIntervalSince(now)
        if seconds <= 0 { return .now }
        if seconds < 60 { return .after(seconds) }
        var components = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: at)
        components.timeZone = calendar.timeZone
        return .on(components)
    }
}

/// The tap round trip: post stores the tap JSON in the notification's
/// userInfo; the host's `UNUserNotificationCenterDelegate` hands the response
/// back here and sends the JSON VERBATIM to the app instance (`event(tap)`).
public enum NotificationTaps {
    public static let idKey = "at.exponential.engine.id"
    public static let tapKey = "at.exponential.engine.tap"

    public static func userInfo(id: String, tapJson: String?) -> [String: String] {
        var info = [idKey: id]
        if let tapJson { info[tapKey] = tapJson }
        return info
    }

    /// The tap JSON (`{type: notificationTap, app, screen, params}`), nil for a
    /// notification without one or not posted by the Engine.
    public static func tapJson(userInfo: [AnyHashable: Any]) -> String? {
        userInfo[tapKey] as? String
    }

    /// The Engine's id (`<app>/<id>`) of a notification it posted.
    public static func engineId(userInfo: [AnyHashable: Any]) -> String? {
        userInfo[idKey] as? String
    }
}

/// `notifications`: post (now or scheduled; replaces the same id), cancel.
public final class NotificationsCapability: Capability, @unchecked Sendable {
    let scheduler: NotificationScheduler?
    let now: () -> Date
    let calendar: Calendar
    let runner: Runner

    public init(scheduler: NotificationScheduler?, now: @escaping () -> Date = Date.init, calendar: Calendar = .current,
                isMainThread: @escaping () -> Bool = { Thread.isMainThread }) {
        self.scheduler = scheduler
        self.now = now
        self.calendar = calendar
        runner = Runner(capability: "notifications", isMainThread: isMainThread)
    }

    public func name() -> String { "notifications" }
    public func toolsJson() -> String { ToolCatalog.json("notifications") }

    public func call(tool: String, argumentsJson: String) throws -> String {
        runner.run(tool, argumentsJson) { tool, args in
            guard ["post", "cancel"].contains(tool) else { throw runner.unknown(tool) }
            guard let scheduler else { throw CapabilityFailure.unavailable }
            let id = try args.string("id")
            if tool == "cancel" {
                let existed = try scheduler.pendingIds().contains(id) || scheduler.deliveredIds().contains(id)
                try scheduler.remove(ids: [id])
                return ["cancelled": existed]
            }
            let title = try args.string("title")
            let body = try args.optionalString("body")
            let at = try args.optionalString("at").map { text -> Date in
                guard let date = RFC3339.parse(text) else { throw CapabilityFailure.failed("arguments: at is not an RFC 3339 time") }
                return date
            }
            let tap = try args.object("tap").map { JSON.text($0) }
            var allowed: Bool
            switch try scheduler.authorization() {
            case .granted: allowed = true
            case .denied: allowed = false
            case .notDetermined: allowed = try scheduler.requestAuthorization()
            }
            guard allowed else { throw CapabilityFailure.failed("notifications not allowed") }
            try scheduler.remove(ids: [id])
            try scheduler.schedule(PlannedNotification(
                id: id, title: title, body: body,
                trigger: NotificationPlan.trigger(at: at, now: now(), calendar: calendar),
                userInfo: NotificationTaps.userInfo(id: id, tapJson: tap)))
            return [:]
        }
    }
}
