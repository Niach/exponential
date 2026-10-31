import Foundation

/// VAPP-110: Exponential APPS ×4 (`packages/domain-contract/fixtures/apps.json`,
/// TS reference `src/apps.ts`; desktop `domain::apps`, Android
/// `domain/Apps.kt`). An app = a team-scoped declarative Exponential UI
/// package rendered through the client's app host. The render rules live in
/// the fixture's `$comment`; `AppsTests` replays `slugifyCases`,
/// `versionLabelCases` and `sortAppsCases` and pins the constants below to
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

        public static func viewing(_ number: Int) -> String {
            viewing.replacingOccurrences(of: "{n}", with: String(number))
        }

        public static func restored(_ restored: Int, as version: Int) -> String {
            self.restored
                .replacingOccurrences(of: "{n}", with: String(restored))
                .replacingOccurrences(of: "{m}", with: String(version))
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
        // VAPP-178: the Data tab's body.
        public static let data = "app-editor-data"
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
            "An app is a declarative Exponential UI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: \"get\", slug: \"\(slug)\"}; learn the components, bindings and functions with exponential_apps {op: \"catalog\"}; publish every change as a new version with exponential_apps {op: \"upsert\", slug: \"\(slug)\", package, note}. The person sees the live preview beside this chat, so keep replies to one or two lines and publish early.",
        ]
        return lines.joined(separator: "\n")
    }

    /// The first template id of a package's JSON text (the editor renders
    /// it); nil = not a package or no template. "First" = the key's order in
    /// the TEXT, as the TS `Object.keys` of the parsed synced row.
    public static func firstTemplateId(packageJSON: String) -> String? {
        guard let data = packageJSON.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let templates = object["templates"] as? [String: Any],
              !templates.isEmpty,
              let section = packageJSON.range(of: "\"templates\"")
        else { return nil }
        let tail = packageJSON[section.upperBound...]
        return templates.keys.min { a, b in
            let ia = tail.range(of: "\"\(a)\"")?.lowerBound ?? tail.endIndex
            let ib = tail.range(of: "\"\(b)\"")?.lowerBound ?? tail.endIndex
            return ia != ib ? ia < ib : a < b
        }
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
