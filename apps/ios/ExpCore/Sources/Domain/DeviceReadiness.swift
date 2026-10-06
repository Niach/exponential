import Foundation

// EXP-1196/1218/1219: THE device readiness block, one spec ×5 — the contract
// fixture `packages/domain-contract/fixtures/device-doctor.json` (locked by
// `DeviceReadinessTests`). A device reports `doctor` on register + heartbeat;
// every client renders the SAME block from it: groups as filled bands, one
// flat row per item (state glyph, label, detail, at most one action). The
// device writes `detail`; clients never compose it. No subtitles, no footers.

// MARK: - Wire model

/// The synced `devices.doctor` blob. Lenient: unknown keys, groups, states
/// and actions stay plain strings (a newer device must never blank the
/// block), and one malformed item drops alone.
public struct DeviceDoctor: Decodable, Sendable, Equatable {
    public let checkedAt: String?
    public let items: [Item]

    public struct Item: Decodable, Sendable, Equatable {
        public let key: String
        public let group: String
        public let parent: String?
        public let state: String
        public let detail: String?
        public let action: String?

        public init(
            key: String, group: String, parent: String? = nil, state: String,
            detail: String? = nil, action: String? = nil
        ) {
            self.key = key
            self.group = group
            self.parent = parent
            self.state = state
            self.detail = detail
            self.action = action
        }
    }

    public init(checkedAt: String? = nil, items: [Item]) {
        self.checkedAt = checkedAt
        self.items = items
    }

    private enum CodingKeys: String, CodingKey { case checkedAt, items }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        checkedAt = try? c.decodeIfPresent(String.self, forKey: .checkedAt)
        items = ((try? c.decodeIfPresent([FailableItem].self, forKey: .items)) ?? nil)?
            .compactMap(\.value) ?? []
    }

    /// The stored column (stringified JSON) as a report; nil for NULL and
    /// for anything that is not an object.
    public static func decode(json: String?) -> DeviceDoctor? {
        guard let json, let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(DeviceDoctor.self, from: data)
    }
}

/// An item that never throws: no `key` = dropped; a missing group/state
/// reads as empty (an unknown group / a muted dash), never a lost report.
private struct FailableItem: Decodable {
    let value: DeviceDoctor.Item?

    private enum CodingKeys: String, CodingKey { case key, group, parent, state, detail, action }

    init(from decoder: Decoder) throws {
        guard let c = try? decoder.container(keyedBy: CodingKeys.self),
              let key = try? c.decode(String.self, forKey: .key), !key.isEmpty
        else {
            value = nil
            return
        }
        func string(_ k: CodingKeys) -> String? {
            (try? c.decodeIfPresent(String.self, forKey: k)) ?? nil
        }
        value = DeviceDoctor.Item(
            key: key,
            group: string(.group) ?? "",
            parent: string(.parent),
            state: string(.state) ?? "",
            detail: string(.detail),
            action: string(.action)
        )
    }
}

// MARK: - Presentation

public enum DeviceReadiness {
    public enum Glyph: String, Sendable { case check, alert, dash, x }
    public enum Tone: String, Sendable { case success, warning, muted, destructive }

    /// Group order + labels + tag (fixture `groups`; the band shows the tag
    /// VERBATIM, lowercase `optional`). Unknown groups follow,
    /// in report order, labelled by their key.
    public static let groups: [(key: String, label: String, tag: String?)] = [
        ("required", "Required", nil),
        ("agents", "Coding agents", "optional"),
        ("computer_use", "Computer use", "optional"),
    ]

    /// Fixture `labels`; an unknown key renders verbatim.
    public static let labels: [String: String] = [
        "git": "Git",
        "claude": "Claude Code",
        "codex": "Codex",
        "computer_use": "Computer use",
        "screen_recording": "Screen Recording",
        "accessibility": "Accessibility",
    ]

    /// Fixture `states`; an unknown state is a muted dash.
    public static let states: [String: (glyph: Glyph, tone: Tone)] = [
        "ok": (.check, .success),
        "action": (.alert, .warning),
        "missing": (.dash, .muted),
        "off": (.dash, .muted),
        "error": (.x, .destructive),
    ]

    /// Fixture `actions`; an unknown action is local-only, labelled by key.
    public static let actions: [String: (label: String, remote: Bool)] = [
        "install": ("Install", false),
        "update": ("Update", true),
        "sign_in": ("Sign in", true),
        "grant": ("Open System Settings", false),
    ]

    /// The item that IS the switch row (writes `launch_defaults.computerUse`).
    public static let switchKey = "computer_use"

    public struct Row: Equatable, Sendable, Identifiable {
        public let key: String
        public let label: String
        public let state: String
        /// Nil on the switch row (label + switch only).
        public let detail: String?
        public let glyph: Glyph?
        public let tone: Tone?
        /// An OS permission row under its parent.
        public let indented: Bool
        public let isSwitch: Bool
        public let switchOn: Bool
        /// The trailing pill's action key — set only when it is OFFERED here
        /// (a non-remote action on another device renders no pill).
        public let action: String?
        public let actionLabel: String?
        /// The ONE filled pill of the block.
        public let primary: Bool

        public var id: String { key }
    }

    public struct Group: Equatable, Sendable, Identifiable {
        public let key: String
        public let label: String
        public let tag: String?
        public let rows: [Row]

        public var id: String { key }
    }

    /// The block for one device. `remote` = it is ANOTHER device (a phone is
    /// always remote): only `remote: true` actions get a pill.
    /// `computerUseOn` overrides the switch's state with the caller's draft
    /// (the report only catches up on the next heartbeat).
    public static func groups(
        _ doctor: DeviceDoctor,
        remote: Bool,
        computerUseOn: Bool? = nil
    ) -> [Group] {
        let items = doctor.items
        var offState: [String: Bool] = [:]
        for item in items {
            var off = item.state == "off"
            if item.key == switchKey, let computerUseOn { off = !computerUseOn }
            offState[item.key] = off
        }
        let visible = items.filter { item in
            guard let parent = item.parent else { return true }
            return offState[parent] != true
        }
        // The primary pill: the first `action`/`error` row with an offered
        // action, in block (= group) order.
        var order = groups.map(\.key)
        for item in visible where !order.contains(item.group) { order.append(item.group) }
        let ordered = order.flatMap { key in visible.filter { $0.group == key } }
        let primaryKey = ordered.first { item in
            (item.state == "action" || item.state == "error")
                && offeredAction(item, remote: remote) != nil
        }?.key

        return order.compactMap { key in
            let rows = ordered.filter { $0.group == key }.map {
                row($0, remote: remote, primary: $0.key == primaryKey, switchOff: offState[$0.key] == true)
            }
            guard !rows.isEmpty else { return nil }
            let known = groups.first { $0.key == key }
            return Group(key: key, label: known?.label ?? key, tag: known?.tag, rows: rows)
        }
    }

    /// The agents a report says can run: every REQUIRED item ok, then each
    /// agent row that is ok (fixture `runnable`).
    public static func runnableAgents(_ doctor: DeviceDoctor) -> [String] {
        let required = doctor.items.filter { $0.group == "required" }
        guard required.allSatisfy({ $0.state == "ok" }) else { return [] }
        return doctor.items.filter { $0.group == "agents" && $0.state == "ok" }.map(\.key)
    }

    /// The composer's ONE row: when [agent] cannot run on the device, the
    /// failing required row (Git) if any, else that agent's row; nil when it
    /// can run or the report does not name it. Its pill (if offered) is the
    /// filled one — it is the only action on screen.
    public static func failingRow(_ doctor: DeviceDoctor, agent: String, remote: Bool) -> Row? {
        if let required = doctor.items.first(where: { $0.group == "required" && $0.state != "ok" }) {
            return row(required, remote: remote, primary: true, switchOff: false)
        }
        guard let item = doctor.items.first(where: { $0.key == agent }), item.state != "ok" else {
            return nil
        }
        return row(item, remote: remote, primary: true, switchOff: false)
    }

    /// The rows that need attention (`action`/`error`), in block order, the
    /// switch row excluded — the device setup block's status line.
    public static func attentionRows(_ doctor: DeviceDoctor, remote: Bool) -> [Row] {
        groups(doctor, remote: remote).flatMap(\.rows).filter {
            !$0.isSwitch && ($0.state == "action" || $0.state == "error")
        }
    }

    private static func offeredAction(_ item: DeviceDoctor.Item, remote: Bool) -> String? {
        guard let action = item.action, !action.isEmpty else { return nil }
        let remoteOk = actions[action]?.remote ?? false
        return (!remote || remoteOk) ? action : nil
    }

    private static func row(
        _ item: DeviceDoctor.Item, remote: Bool, primary: Bool, switchOff: Bool
    ) -> Row {
        let isSwitch = item.key == switchKey
        let presentation = states[item.state] ?? (.dash, .muted)
        let action = isSwitch ? nil : offeredAction(item, remote: remote)
        return Row(
            key: item.key,
            label: labels[item.key] ?? item.key,
            state: item.state,
            detail: isSwitch ? nil : item.detail,
            glyph: isSwitch ? nil : presentation.glyph,
            tone: isSwitch ? nil : presentation.tone,
            indented: item.parent != nil,
            isSwitch: isSwitch,
            switchOn: isSwitch && !switchOff,
            action: action,
            actionLabel: action.map { actions[$0]?.label ?? $0 },
            primary: primary && action != nil
        )
    }
}
