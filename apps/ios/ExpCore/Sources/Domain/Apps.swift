import Foundation

/// VAPP-110: Exponential APPS ×4 (`packages/domain-contract/fixtures/apps.json`,
/// TS reference `src/apps.ts`; desktop `domain::apps`, Android
/// `domain/Apps.kt`). An app = a team-scoped declarative Exponential UI
/// package rendered through the client's app host. The render rules live in
/// the fixture's `$comment`; `AppsTests` replays `slugifyCases`,
/// `versionLabelCases`, `sortAppsCases`, `publishCases`, `automationRowCases` and `webhookRowCases` and pins the constants below to
/// the fixture and `contract.json` `apps`.
public enum Apps {
    // MARK: - Fixture constants

    public enum Copy {
        public static let nav = "Apps"
        public static let title = "Apps"
        public static let newApp = "New app"
        /// VAPP-176: a New app DRAFT's name until the editor session names it.
        public static let untitled = "Untitled app"
        /// VAPP-176: the list band's Import pill (before New app).
        public static let `import` = "Import\u{2026}"
        public static let version = "v{n}"
        public static let noVersion = "Draft"
        public static let versions = "Versions"
        public static let head = "Current"
        public static let noNote = "No note"
        public static let byAgent = "the editor session"
        public static let viewing = "Viewing v{n}"
        public static let restore = "Restore"
        public static let restored = "Restored v{n} as v{m}"
        public static let restoredNote = "Restored v{n}"
        /// eApps V1 (#VAPP-210): Publish, the published row's badge, its toast
        /// and the server's refusal while nothing exists.
        public static let publish = "Publish"
        public static let published = "Published"
        public static let publishedToast = "Published v{n}"
        public static let publishNothing = "Nothing to publish yet"
        /// eApps R8 (#VAPP-233, `migrationCases`): a version row's record sweep.
        public static let migrating = "Migrating records"
        public static let migratingOf = "Migrating records {done} of {total}"
        public static let migrationFailed = "Records not migrated: {error}"
        public static let empty = "No apps yet"
        public static let chatEmpty = "Describe the app you want. The editor session builds it and the preview follows."
        public static let previewEmpty = "No version yet"
        public static let versionsEmpty = "No versions yet"
        public static let composerPlaceholder = "Ask for a change…"
        /// VAPP-176: a draft's composer and its starter empty state.
        public static let draftPlaceholder = "Describe your app…"
        public static let starterTitle = "What should this app do?"
        public static let starters = [
            "Log working hours",
            "Release checklist",
            "Admin console for an API",
            "Team dashboard",
        ]
        public static let noDevice = "Start a device to edit this app"
        public static let namePlaceholder = "Name"
        public static let descriptionPlaceholder = "What is it for? (optional)"
        public static let deleteTitle = "Delete {name}?"
        public static let deleteBody = "Its versions go with it. This cannot be undone."
        public static let delete = "Delete"
        public static let settingsTitle = "Apps"
        // VAPP-122: an app's records (the editor's Settings tab / sheet).
        public static let records = "{n} records"
        public static let recordsOne = "1 record"
        public static let recordsOf = "{n} of {max} records"
        public static let recordsNone = "No records"
        /// VAPP-177: a Versions row's ghost button (shows that version).
        public static let preview = "Preview"

        /// VAPP-177: the list row's `…` and the phone editor's `…` menu.
        public enum Menu {
            public static let versions = "Versions"
            public static let settings = "Settings"
            public static let export = "Export"
            public static let delete = "Delete app"
        }

        public enum Tabs {
            public static let chat = "Chat"
            public static let preview = "Preview"
            /// VAPP-178: the records browser (app-records.json `dataTab`).
            public static let data = "Data"
            public static let versions = "Versions"
            public static let settings = "Settings"
        }

        /// #VAPP-220: the Settings tab's Automations group (`automationRow`).
        public enum Automations {
            public static let title = "Automations"
            public static let schedule = "Schedule {cron}"
            public static let scheduleZone = "Schedule {cron} ({tz})"
            public static let create = "When a {kind} is created"
            public static let update = "When a {kind} is updated"
            public static let delete = "When a {kind} is deleted"
            public static let event = "On {event}"
            public static let webhook = "On webhook {name}"
            public static let paused = "Paused"
            public static let failed = "Failed"
            public static let ok = "OK"
            public static let never = "Not run yet"
        }

        /// #VAPP-238: the Settings tab's Webhooks group (`webhookRow`).
        public enum Webhooks {
            public static let title = "Webhooks"
            public static let ready = "Ready"
            public static let unsigned = "No secret"
            public static let copyUrl = "Copy URL"
            public static let copied = "Copied"
            public static let mint = "Create secret"
            public static let rotate = "Rotate secret"
            public static let secret = "New secret"
            public static let copySecret = "Copy secret"
        }

        public static func viewing(_ number: Int) -> String {
            viewing.replacingOccurrences(of: "{n}", with: String(number))
        }

        public static func restored(_ restored: Int, as version: Int) -> String {
            self.restored
                .replacingOccurrences(of: "{n}", with: String(restored))
                .replacingOccurrences(of: "{m}", with: String(version))
        }

        public static func publishedToast(_ number: Int) -> String {
            publishedToast.replacingOccurrences(of: "{n}", with: String(number))
        }

        public static func deleteTitle(_ name: String) -> String {
            deleteTitle.replacingOccurrences(of: "{name}", with: name)
        }
    }

    public enum Panes {
        public static let chat: Double = 5
        public static let preview: Double = 7
        public static let chatMinWidth: Double = 360
        public static let breakpoint: Double = 768
    }

    public enum Preview {
        public static let radius: Double = 12
        public static let padding: Double = 16
        public static let surfaceId = "app"
    }

    public enum TestIds {
        public static let list = "apps-list"
        public static let row = "apps-row"
        public static let newApp = "apps-new"
        public static let editor = "app-editor"
        public static let chat = "app-editor-chat"
        public static let preview = "app-editor-preview"
        public static let versions = "app-editor-versions"
        public static let versionRow = "app-version-row"
        public static let tabs = "app-editor-tabs"
        public static let composer = "app-editor-composer"
        public static let viewing = "app-editor-viewing"
        // VAPP-176: the band's Import pill, a draft's starter pills.
        public static let importApp = "apps-import"
        public static let starter = "app-editor-starter"
        // VAPP-177: the phone editor's `…` menu, the Settings body, a
        // Versions row's Preview / Restore.
        public static let menu = "app-editor-menu"
        public static let settings = "app-editor-settings"
        public static let versionPreview = "app-version-preview"
        public static let versionRestore = "app-version-restore"
        // eApps V1: the editor's Publish, a Versions row's Publish.
        public static let publish = "app-editor-publish"
        public static let versionPublish = "app-version-publish"
        // VAPP-178: the Data tab's body.
        public static let data = "app-editor-data"
        // VAPP-179: the Preview toolbar (web ≥ md + desktop only; phones
        // have none, the ids stay in step with the fixture).
        public static let previewToolbar = "app-preview-toolbar"
        public static let previewFrame = "app-preview-frame"
        public static let previewTemplate = "app-preview-template"
        // VAPP-198: Run as (in the toolbar: web ≥ md + desktop only).
        public static let previewRunAs = "app-preview-run-as"
        // #VAPP-220: the Settings tab's Automations group and its rows.
        public static let automations = "app-settings-automations"
        public static let automationRow = "app-automation-row"
        // #VAPP-238: the Settings tab's Webhooks group, its rows and the
        // shown-once secret row.
        public static let webhooks = "app-settings-webhooks"
        public static let webhookRow = "app-webhook-row"
        public static let webhookSecret = "app-webhook-secret"
        // VAPP-218: the preview's screens navigation (the bar on phones).
        public static let previewScreens = "app-editor-preview-screens"
    }

    /// VAPP-177 the editor's tabs (fixture `editorTabs`): web ≥ md and
    /// desktop = Preview | Data | Versions | Settings beside the chat;
    /// phones = Chat | Preview | Data (VAPP-178) + the `…` menu, whose
    /// Versions and Settings open the same bodies as sheets.
    public enum EditorTabs {
        public static let wide = ["preview", "data", "versions", "settings"]
        public static let phone = ["chat", "preview", "data"]
        public static let menu = ["versions", "settings", "export", "delete"]
        public static let `default` = "preview"
        public static let phoneDefault = "chat"
        public static let settingsMaxWidth: Double = 640
        public static let search = "tab"
    }

    // MARK: - Contract caps (`contract.json` `apps`)

    public static let slugMax = 64
    public static let nameMax = 64
    public static let descriptionMax = 2000
    public static let noteMax = 500
    public static let packageMaxBytes = 131_072
    public static let templateMaxComponents = 500
    public static let slugPattern = "^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$"
    /// VAPP-122: a record kind's name, one record's `data` JSON cap, the
    /// kinds one manifest may declare.
    public static let kindPattern = "^[a-z][a-z0-9_]{0,31}$"
    public static let recordMaxBytes = 16_384
    public static let recordKindsMax = 16

    // MARK: - VAPP-157 the app hosts' media rule (fixture `mediaPolicy`)

    /// The fixture's `dataMimePrefixes`, `deniedDataMimes` and
    /// `attachmentPath` (`AppsTests` locks them).
    public static let mediaDataMimePrefixes = ["image/", "audio/", "video/"]
    public static let mediaDeniedDataMimes = ["image/svg+xml"]
    public static let mediaAttachmentPath = "/api/attachments/"

    /// The TS `appMediaAllowed` byte for byte (a plain string rule over
    /// UTF-16 units, like the JS): a `data:` image, audio or video (never
    /// svg), or the instance's own `/api/attachments/<uuid>` (relative or on
    /// `origin` exactly) with nothing after the id. Applied to the RAW src.
    public static func appMediaAllowed(_ src: String, origin: String) -> Bool {
        if src.isEmpty || jsTrim(src) != src { return false }
        let lower = src.lowercased()
        if lower.hasPrefix("data:") {
            guard let comma = lower.firstIndex(of: ",") else { return false }
            let header = lower[lower.index(lower.startIndex, offsetBy: 5)..<comma]
            let mime = String(header.split(separator: ";", omittingEmptySubsequences: false).first ?? "")
            return mediaDataMimePrefixes.contains { mime.hasPrefix($0) && mime.utf16.count > $0.utf16.count }
                && !mediaDeniedDataMimes.contains(mime)
        }
        let path: [UInt16]
        if src.hasPrefix("/"), !src.hasPrefix("//") {
            path = Array(src.utf16)
        } else {
            var trimmed = origin
            while trimmed.hasSuffix("/") { trimmed.removeLast() }
            let base = trimmed.lowercased()
            if base.isEmpty || !lower.hasPrefix(base + "/") { return false }
            path = Array(src.utf16.dropFirst(base.utf16.count))
        }
        return attachmentPathMatches(path)
    }

    /// `^<attachmentPath><uuid>$`, ASCII case-insensitive (the JS `i` flag
    /// never folds a non-ASCII unit onto ASCII).
    private static func attachmentPathMatches(_ path: [UInt16]) -> Bool {
        let prefix = Array(mediaAttachmentPath.utf16)
        guard path.count == prefix.count + 36 else { return false }
        func fold(_ u: UInt16) -> UInt16 { (0x41...0x5A).contains(u) ? u + 0x20 : u }
        for (i, u) in prefix.enumerated() where fold(path[i]) != fold(u) { return false }
        for (i, u) in path.dropFirst(prefix.count).enumerated() {
            if [8, 13, 18, 23].contains(i) {
                if u != 0x2D { return false }
            } else {
                let f = fold(u)
                if !((0x30...0x39).contains(f) || (0x61...0x66).contains(f)) { return false }
            }
        }
        return true
    }

    /// JS `String.prototype.trim`'s set: whitespace, line terminators and
    /// the BOM.
    private static func jsTrim(_ value: String) -> String {
        value.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines.union(CharacterSet(charactersIn: "\u{FEFF}")))
    }

    // MARK: - VAPP-221 the app hosts' FileUpload uploads (fixture `appFiles`)

    /// The fixture's `uploadPath` (`{appId}` = the app), `partName` and the
    /// failed-action toast's `failedVerb` / `failedThing` (`AppsTests`
    /// locks them): every picked file POSTs to the upload path as one
    /// multipart part; the answer's `url` lands in the FileUpload's bound list.
    public enum Files {
        public static let uploadPath = "/api/apps/{appId}/files"
        public static let partName = "file"
        public static let failedVerb = "upload"
        public static let failedThing = "the file"
    }

    /// The upload url path of an app (TS `appUploadPath`: `uploadPath` with
    /// the id as JS `encodeURIComponent`).
    public static func appUploadPath(_ appId: String) -> String {
        var allowed = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
        allowed.insert(charactersIn: "-_.!~*'()")
        let encoded = appId.addingPercentEncoding(withAllowedCharacters: allowed) ?? appId
        return Files.uploadPath.replacingOccurrences(of: "{appId}", with: encoded)
    }

    /// The absolute `files` path of the FIRST FileUpload (component order)
    /// named `name` (its `name`, else its id); nil when it has no absolute
    /// `{path}` binding (TS `appUploadTarget`, `targetCases`).
    public static func appUploadTarget(_ components: [CardJSON], name: String) -> String? {
        let upload = components.first { component in
            guard let object = component.objectValue, object["component"] == .string("FileUpload") else { return false }
            let named: CardJSON? = {
                if case .string? = object["name"] { return object["name"] }
                return object["id"]
            }()
            return named == .string(name)
        }
        guard case let .string(path)? = upload?.objectValue?["files"]?.objectValue?["path"], path.hasPrefix("/") else {
            return nil
        }
        return path
    }

    /// `list` with `url` on every object entry named `fileName`; nil =
    /// nothing to write (no list, no such entry) (TS `withUploadedUrl`,
    /// `urlCases`).
    public static func withUploadedUrl(_ list: CardJSON?, fileName: String, url: String) -> [CardJSON]? {
        guard case let .array(entries)? = list else { return nil }
        var hit = false
        let next: [CardJSON] = entries.map { entry in
            guard case var .object(object) = entry, object["name"] == .string(fileName) else { return entry }
            hit = true
            object["url"] = .string(url)
            return .object(object)
        }
        return hit ? next : nil
    }

    /// The value at a JSON pointer (`/a/0/b`, RFC 6901 `~1` / `~0`) of
    /// `root`; nil = nothing there. "" = the root.
    public static func valueAt(pointer: String, in root: CardJSON) -> CardJSON? {
        if pointer.isEmpty { return root }
        guard pointer.hasPrefix("/") else { return nil }
        var current: CardJSON? = root
        for raw in pointer.dropFirst().split(separator: "/", omittingEmptySubsequences: false) {
            let token = raw.replacingOccurrences(of: "~1", with: "/").replacingOccurrences(of: "~0", with: "~")
            switch current {
            case let .object(object)?:
                current = object[token]
            case let .array(items)?:
                guard let index = Int(token), index >= 0, index < items.count else { return nil }
                current = items[index]
            default:
                return nil
            }
        }
        return current
    }

    // MARK: - The model

    /// VAPP-176: a DRAFT = an app with no version yet (New app's
    /// "Untitled app" until the editor session publishes).
    public static func isDraft(headVersion: Int) -> Bool { headVersion == 0 }

    /// The tab on screen and, on phones, the menu sheet over it.
    public struct EditorTabTarget: Equatable, Sendable {
        public let tab: String
        public let sheet: String?

        public init(tab: String, sheet: String?) {
            self.tab = tab
            self.sheet = sheet
        }
    }

    /// TS `editorTab` (`editorTabCases`): a requested tab → the tab on
    /// screen; a phone opens `versions` / `settings` as the menu's sheet over
    /// Chat; anything unknown = the form's default.
    public static func editorTab(requested: String?, phone: Bool) -> EditorTabTarget {
        if phone {
            if let requested, requested == "versions" || requested == "settings" {
                return EditorTabTarget(tab: EditorTabs.phoneDefault, sheet: requested)
            }
            if let requested, EditorTabs.phone.contains(requested) {
                return EditorTabTarget(tab: requested, sheet: nil)
            }
            return EditorTabTarget(tab: EditorTabs.phoneDefault, sheet: nil)
        }
        if let requested, EditorTabs.wide.contains(requested) {
            return EditorTabTarget(tab: requested, sheet: nil)
        }
        return EditorTabTarget(tab: EditorTabs.default, sheet: nil)
    }

    /// `v{n}`, or the "no version" copy while an app has no head.
    public static func versionLabel(_ headVersion: Int) -> String {
        headVersion > 0
            ? Copy.version.replacingOccurrences(of: "{n}", with: String(headVersion))
            : Copy.noVersion
    }

    // MARK: - eApps V1 draft vs published (#VAPP-210, fixture `publishCases`)

    /// An app's two pointers.
    public struct Versions: Equatable, Sendable {
        public let headVersion: Int
        public let publishedVersion: Int

        public init(headVersion: Int, publishedVersion: Int) {
            self.headVersion = headVersion
            self.publishedVersion = publishedVersion
        }
    }

    /// A Versions row's trailing (TS `versionRow`).
    public enum VersionRowKind: String, Equatable, Sendable {
        case published, head, version
    }

    /// The version the editor's preview shows: an explicitly viewed one, else
    /// nothing (no version), else the HEAD (the draft) while the viewer's own
    /// editor session is live or nothing is published, else the PUBLISHED one.
    public static func shownVersion(_ app: Versions, editing: Bool, viewing: Int?) -> Int {
        if let viewing { return viewing }
        if app.headVersion == 0 { return 0 }
        return editing || app.publishedVersion == 0 ? app.headVersion : app.publishedVersion
    }

    /// Publish is offered while the head is not the published version.
    public static func canPublish(_ app: Versions) -> Bool {
        app.headVersion > 0 && app.headVersion != app.publishedVersion
    }

    /// The list's and the header's version pill: the PUBLISHED version.
    public static func listPill(_ app: Versions) -> String {
        versionLabel(app.publishedVersion)
    }

    /// The version a grant is made on and checked against (`apps.update
    /// {headVersion}` carries it): the published one, else the head.
    public static func grantVersion(_ app: Versions) -> Int {
        app.publishedVersion > 0 ? app.publishedVersion : app.headVersion
    }

    /// A Versions row's trailing: the published badge, the head badge +
    /// Publish, or Preview + Publish.
    /// eApps R8 (#VAPP-233, fixture `migrationCases`): a version row's sweep
    /// line after its subtitle, or nil (no sweep, done, superseded).
    public static func versionMigration(state: String?, total: Int, done: Int, error: String?) -> String? {
        switch state {
        case "running":
            return total > 0 ? Copy.migratingOf.replacingOccurrences(of: "{done}", with: String(done)).replacingOccurrences(of: "{total}", with: String(total)) : Copy.migrating
        case "failed":
            return Copy.migrationFailed.replacingOccurrences(of: "{error}", with: error ?? "")
        default:
            return nil
        }
    }

    public static func versionRow(_ app: Versions, number: Int) -> VersionRowKind {
        if number == app.publishedVersion { return .published }
        return number == app.headVersion ? .head : .version
    }

    // MARK: - eApps automations (#VAPP-220, fixture `automationRowCases`)

    /// An automation's trigger (tRPC `appAccess.appTools` `automations[].on`).
    public enum AutomationTrigger: Equatable, Sendable {
        case schedule(cron: String, tz: String?)
        /// `op` = create | update | delete.
        case record(kind: String, op: String)
        case event(String)
        /// #VAPP-237: a webhook delivery (the hook name).
        case webhook(String)
    }

    /// One row of the Settings tab's Automations group (TS `AutomationRowInput`).
    public struct AutomationRowInput: Equatable, Sendable {
        public let name: String
        public let on: AutomationTrigger
        public let paused: String?
        public let lastStatus: String?

        public init(name: String, on: AutomationTrigger, paused: String?, lastStatus: String?) {
            self.name = name
            self.on = on
            self.paused = paused
            self.lastStatus = lastStatus
        }
    }

    public enum AutomationStatus: String, Equatable, Sendable {
        case paused, failed, ok, never
    }

    public struct AutomationRow: Equatable, Sendable {
        public let trigger: String
        public let status: AutomationStatus
        public let label: String
    }

    /// The row's second line, its status and the trailing pill's label (TS
    /// `automationRow`): a pause wins, then no run yet, then the LAST run's
    /// status (anything but `ok` reads as failed).
    public static func automationRow(_ row: AutomationRowInput) -> AutomationRow {
        typealias Strings = Apps.Copy.Automations
        let trigger: String
        switch row.on {
        case let .schedule(cron, tz):
            if let tz, !tz.isEmpty {
                trigger = Strings.scheduleZone
                    .replacingOccurrences(of: "{cron}", with: cron)
                    .replacingOccurrences(of: "{tz}", with: tz)
            } else {
                trigger = Strings.schedule.replacingOccurrences(of: "{cron}", with: cron)
            }
        case let .record(kind, op):
            let template: String
            switch op {
            case "update": template = Strings.update
            case "delete": template = Strings.delete
            default: template = Strings.create
            }
            trigger = template.replacingOccurrences(of: "{kind}", with: kind)
        case let .event(event):
            trigger = Strings.event.replacingOccurrences(of: "{event}", with: event)
        case let .webhook(name):
            trigger = Strings.webhook.replacingOccurrences(of: "{name}", with: name)
        }
        let status: AutomationStatus
        if let paused = row.paused, !paused.isEmpty {
            status = .paused
        } else if let last = row.lastStatus {
            status = last == "ok" ? .ok : .failed
        } else {
            status = .never
        }
        let label: String
        switch status {
        case .paused: label = Strings.paused
        case .failed: label = Strings.failed
        case .ok: label = Strings.ok
        case .never: label = Strings.never
        }
        return AutomationRow(trigger: trigger, status: status, label: label)
    }

    // MARK: - eApps webhooks (#VAPP-238, fixture `webhookRowCases`)

    /// One row of the Settings tab's Webhooks group (TS `WebhookRowInput`),
    /// from tRPC `appAccess.appTools` `hooks`.
    public struct WebhookRowInput: Equatable, Sendable {
        public let name: String
        public let hasSecret: Bool

        public init(name: String, hasSecret: Bool) {
            self.name = name
            self.hasSecret = hasSecret
        }
    }

    public enum WebhookStatus: String, Equatable, Sendable {
        case ready, unsigned
    }

    public enum WebhookAction: String, Equatable, Sendable {
        case mint, rotate
    }

    public struct WebhookRow: Equatable, Sendable {
        public let status: WebhookStatus
        public let label: String
        public let action: WebhookAction?
        public let actionLabel: String?
    }

    /// The trailing pill and the owner's action pill (TS `webhookRow`;
    /// `isOwner` = `appTools.canToggle`): no secret yet = mint, else rotate.
    public static func webhookRow(_ row: WebhookRowInput, isOwner: Bool) -> WebhookRow {
        typealias Strings = Apps.Copy.Webhooks
        let status: WebhookStatus = row.hasSecret ? .ready : .unsigned
        let action: WebhookAction? = isOwner ? (row.hasSecret ? .rotate : .mint) : nil
        let actionLabel: String?
        switch action {
        case .mint: actionLabel = Strings.mint
        case .rotate: actionLabel = Strings.rotate
        case nil: actionLabel = nil
        }
        return WebhookRow(
            status: status,
            label: status == .ready ? Strings.ready : Strings.unsigned,
            action: action,
            actionLabel: actionLabel
        )
    }

    /// A name → its slug: lowercase ASCII letters, digits and single dashes,
    /// diacritics stripped, cut at the cap, `app` when nothing is left; a
    /// taken slug gets `-2`, `-3`, … (TS `slugify`).
    public static func slugify(_ name: String, taken: [String] = []) -> String {
        var dashed = ""
        var pendingDash = false
        let folded = name.decomposedStringWithCompatibilityMapping.lowercased()
        for scalar in folded.unicodeScalars {
            // NFKD combining marks (U+0300–U+036F) vanish, like the TS regex.
            if (0x300...0x36F).contains(scalar.value) { continue }
            let ascii = scalar.value < 128
            let char = Character(scalar)
            if ascii, char.isLetter || char.isNumber {
                if pendingDash, !dashed.isEmpty { dashed.append("-") }
                pendingDash = false
                dashed.append(char)
            } else {
                pendingDash = true
            }
        }
        var base = trimTrailingDashes(String(dashed.prefix(slugMax)))
        if base.isEmpty { base = "app" }
        guard taken.contains(base) else { return base }
        var n = 2
        while true {
            let suffix = "-\(n)"
            let candidate = trimTrailingDashes(String(base.prefix(slugMax - suffix.count))) + suffix
            if !taken.contains(candidate) { return candidate }
            n += 1
        }
    }

    private static func trimTrailingDashes(_ value: String) -> String {
        var out = value
        while out.hasSuffix("-") { out.removeLast() }
        return out
    }

    /// The list order: by name (case- and accent-insensitive, TS
    /// `localeCompare` sensitivity `base`); Exponential ships no built-in apps.
    public static func sortApps<T>(_ apps: [T], name: (T) -> String) -> [T] {
        apps.enumerated().sorted { a, b in
            let order = name(a.element).compare(
                name(b.element), options: [.caseInsensitive, .diacriticInsensitive]
            )
            if order != .orderedSame { return order == .orderedAscending }
            return a.offset < b.offset
        }.map(\.element)
    }

    /// The brief an app's EDITOR SESSION gets as the preface of its first
    /// message (the server writes it; mirrored for parity).
    public static func editorBrief(name: String, slug: String, headVersion: Int, description: String?) -> String {
        let purpose = description?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let lines = [
            "You are the editor session of the Exponential app \"\(name)\" (slug `\(slug)`, \(versionLabel(headVersion))).\(purpose.isEmpty ? "" : " It is for: \(purpose)")",
        ] + (isDraft(headVersion: headVersion) ? [
            "It is a new DRAFT with a placeholder name: on your FIRST exponential_apps {op: \"upsert\"} also pass name, icon, color and description that fit what the person asks for. The slug follows the new name; use the slug the upsert returns from then on.",
        ] : []) + [
            "An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: \"get\", slug: \"\(slug)\"}; learn the components, bindings, host functions and records with exponential_apps {op: \"catalog\"}; save every change as a new draft version with exponential_apps {op: \"upsert\", slug: \"\(slug)\", package, note} (members keep running the published version); publish with exponential_apps {op: \"publish\", slug: \"\(slug)\"} only when the person asks (they also have a Publish button). An app keeps its own data as RECORDS (declare the kinds in the manifest, bind records:<kind>, write with records.upsert); seed or inspect them with exponential_apps {op: \"records_upsert\" | \"records_list\", slug: \"\(slug)\", kind, data}. An app may also EXPOSE MCP tools (exposes: declarative bindings over its records, exponential.<noun>.<op> operations or a team MCP server in upstreams) that a member's MCP client runs as that member; the catalog documents the format. The person sees the live preview beside this chat, so keep replies to one or two lines and save drafts early.",
        ]
        return lines.joined(separator: "\n")
    }

    /// VAPP-179: the preview's template ids, sorted (web `previewTemplates`;
    /// a JSON layer, Postgres JSONB included, does not keep the declared
    /// order).
    public static func previewTemplates(packageJSON: String) -> [String] {
        guard let data = packageJSON.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let templates = object["templates"] as? [String: Any]
        else { return [] }
        return templates.keys.sorted()
    }

    /// The template the preview shows: the picked one while the package has
    /// it, else the first (web `previewTemplate`, `templateCases`); nil = not
    /// a package or no template. Phones have no picker: they pass nil.
    public static func previewTemplate(packageJSON: String, picked: String? = nil) -> String? {
        let list = previewTemplates(packageJSON: packageJSON)
        if let picked, list.contains(picked) { return picked }
        return list.first
    }

    // MARK: - VAPP-218 screens (fixture `screens`, TS `screenShell` …)

    /// The fixture's `screens` constants: the sidebar column's width and the
    /// editor's own query keys (never screen params).
    public enum ScreenSpec {
        public static let sidebarWidth: Double = 200
        public static let reserved = ["tab", "frame"]
    }

    /// Which navigation the preview frame paints for a package's screens.
    public enum ScreenShell: String, Equatable, Sendable {
        case sidebar, bar
    }

    /// A requested screen (a deep link, a tab tap, an `app.navigate`): the
    /// eUI `resolveScreen` turns it into the one shown.
    public struct ScreenRequest: Hashable, Sendable {
        public let screen: String
        public let params: [String: String]

        public init(screen: String, params: [String: String] = [:]) {
            self.screen = screen
            self.params = params
        }
    }

    /// TS `screenShell` (`screenShellCases`): none without screens, the
    /// phone bottom bar on a phone (or the Phone frame), else the sidebar.
    public static func screenShell(screenCount: Int, phone: Bool) -> ScreenShell? {
        if screenCount <= 0 { return nil }
        return phone ? .bar : .sidebar
    }

    /// TS `screenPath` (`screenPathCases`): `/t/<team>/apps/<app>/<screen>`
    /// (`encodeURIComponent` per segment), then the params sorted by key as
    /// `k=v` pairs; none = no `?`.
    public static func screenPath(teamSlug: String, appSlug: String, screen: String, params: [String: String] = [:]) -> String {
        let path = "/t/\(encodeURIComponent(teamSlug))/apps/\(encodeURIComponent(appSlug))/\(encodeURIComponent(screen))"
        let keys = params.keys.sorted()
        if keys.isEmpty { return path }
        let query = keys.map { "\(encodeURIComponent($0))=\(encodeURIComponent(params[$0] ?? ""))" }
        return path + "?" + query.joined(separator: "&")
    }

    /// TS `screenParamsFromQuery` (`screenQueryCases`): every string value
    /// but the editor's own keys (`ScreenSpec.reserved`).
    public static func screenParamsFromQuery(_ query: [String: Any]) -> [String: String] {
        var out: [String: String] = [:]
        for (key, value) in query where !ScreenSpec.reserved.contains(key) {
            if let text = value as? String { out[key] = text }
        }
        return out
    }

    /// JS `encodeURIComponent`'s unreserved set.
    private static let uriComponentAllowed = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()"
    )

    static func encodeURIComponent(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: uriComponentAllowed) ?? value
    }
}

// MARK: - The app host's `exp:` sources (pure, fixture-free)

/// VAPP-110: the `exp:devices` value the app host resolves over the synced
/// store, derived EXACTLY like the web host's `devicesValue`
/// (`apps/web/src/lib/exponential-ui-host.tsx`) and the desktop's
/// `devices_value`. VAPP-119: issues, boards, members, sessions, comments and
/// attachments are fixture-locked in `AppSources`.
public enum AppHostSources {
    /// One `exp:devices` row.
    public struct DeviceRow: Encodable, Equatable, Sendable {
        public let id: String
        public let label: String
        public let detail: String
        public let icon: String
        public let discTone: String
        public let tone: String
        public let status: String
    }

    /// `exp:devices` → `{rows, count, online}`.
    public struct DevicesValue: Encodable, Equatable, Sendable {
        public let rows: [DeviceRow]
        public let count: Int
        public let online: Int
    }

    /// The input a devices row contributes (the synced `devices` row).
    public struct DeviceInput: Sendable {
        public let id: String
        public let label: String
        public let kind: String?
        public let platform: String?
        public let version: String?
        /// The resolved device glyph (`DeviceIconDisplay`).
        public let icon: String
        public let lastSeenAt: String?

        public init(id: String, label: String, kind: String?, platform: String?, version: String?, icon: String, lastSeenAt: String?) {
            self.id = id
            self.label = label
            self.kind = kind
            self.platform = platform
            self.version = version
            self.icon = icon
            self.lastSeenAt = lastSeenAt
        }
    }

    /// `exp:devices` at `now`: rows by label, `Online` within the contract's
    /// window, else `Last seen <ago>`, else `Offline`.
    public static func devicesValue(_ devices: [DeviceInput], now: Date) -> DevicesValue {
        let sorted = devices.sorted { a, b in
            let (la, lb) = (a.label.lowercased(), b.label.lowercased())
            return la != lb ? la < lb : a.label < b.label
        }
        let nowMs = Int64(now.timeIntervalSince1970 * 1000)
        let rows = sorted.map { d -> DeviceRow in
            let online = DeviceLiveness.isOnline(lastSeenAt: d.lastSeenAt, now: now)
            let seen = d.lastSeenAt.flatMap(WireTimestamps.parse)
            let detail = [d.kind == "server" ? "Server" : "Desktop", d.platform, d.version.map { "v\($0)" }]
                .compactMap { $0 }
                .filter { !$0.isEmpty }
                .joined(separator: " · ")
            let status: String
            if online {
                status = "Online"
            } else if let seen {
                status = "Last seen \(CodingReadiness.ago(nowMs: nowMs, thenMs: Int64(seen.timeIntervalSince1970 * 1000)))"
            } else {
                status = "Offline"
            }
            return DeviceRow(
                id: d.id,
                label: d.label,
                detail: detail,
                icon: d.icon,
                discTone: online ? "success" : "muted",
                tone: online ? "live" : "idle",
                status: status
            )
        }
        return DevicesValue(rows: rows, count: rows.count, online: rows.filter { $0.tone == "live" }.count)
    }

    /// A value as JSON text (keys sorted, stable for dedupe).
    public static func json<T: Encodable>(_ value: T) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(value) else { return "null" }
        return String(decoding: data, as: UTF8.self)
    }
}
