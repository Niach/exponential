import Foundation

/// feedback-states.json `starting` (final review): ONE `startId` per DRAFT
/// START, bound to the target device + the draft fingerprint. Minted on the
/// first Start of a draft and reused by every Retry AND every later Start of
/// the SAME draft on the SAME device until a session lands or the draft or
/// device changes (then a fresh id). The server and the device dedupe the
/// id, so a slow device never ends up with two sessions. Mirrored on Android
/// (`domain/DraftStart.kt`).
public struct DraftStart: Equatable, Sendable {

    /// What a draft start is "the same" by: every field of
    /// `starting.fingerprint.fields` (recheck R2). Lists compare as SETS
    /// (order-free), an absent list or map equals an empty one, text compares
    /// verbatim. The attachments are stable per-pick ids, never the uploaded
    /// ones, which a send stamps. The launch options are the WIRE form the
    /// start sends (`SteerStartOptions`), so a pick that never reaches the
    /// device never mints a new id.
    public struct Fingerprint: Equatable, Sendable {
        public let deviceId: String
        public let text: String
        public let issueIds: Set<String>
        public let actionId: String?
        public let actionInputs: [String: String]
        /// The chat's repository (the hidden Chat builtin's `repo` input).
        public let repositoryId: String?
        public let attachments: Set<String>
        public let agent: String?
        public let model: String?
        public let effort: String?
        public let planMode: Bool?
        public let account: String?
        public let mcpServerIds: Set<String>
        public let computerUse: Bool?
        public let ultracode: Bool?
        public let subagentModel: String?
        /// The composer's Resume toggle (an absent one = off).
        public let resume: Bool

        public init(
            deviceId: String,
            text: String,
            issueIds: [String] = [],
            actionId: String? = nil,
            actionInputs: [String: String]? = nil,
            repositoryId: String? = nil,
            attachments: [String] = [],
            agent: String? = nil,
            model: String? = nil,
            effort: String? = nil,
            planMode: Bool? = nil,
            account: String? = nil,
            mcpServerIds: [String]? = nil,
            computerUse: Bool? = nil,
            ultracode: Bool? = nil,
            subagentModel: String? = nil,
            resume: Bool = false
        ) {
            self.deviceId = deviceId
            self.text = text
            self.issueIds = Set(issueIds)
            self.actionId = actionId
            self.actionInputs = actionInputs ?? [:]
            self.repositoryId = repositoryId
            self.attachments = Set(attachments)
            self.agent = agent
            self.model = model
            self.effort = effort
            self.planMode = planMode
            self.account = account
            self.mcpServerIds = Set(mcpServerIds ?? [])
            self.computerUse = computerUse
            self.ultracode = ultracode
            self.subagentModel = subagentModel
            self.resume = resume
        }

        /// The draft with the launch options it would send.
        public init(
            deviceId: String,
            text: String,
            issueIds: [String] = [],
            actionId: String? = nil,
            actionInputs: [String: String]? = nil,
            repositoryId: String? = nil,
            attachments: [String] = [],
            options: SteerStartOptions
        ) {
            self.init(
                deviceId: deviceId,
                text: text,
                issueIds: issueIds,
                actionId: actionId,
                actionInputs: actionInputs,
                repositoryId: repositoryId,
                attachments: attachments,
                agent: options.agent,
                model: options.model,
                effort: options.effort,
                planMode: options.planMode,
                account: options.account,
                mcpServerIds: options.mcpServerIds,
                computerUse: options.computerUse,
                ultracode: options.ultracode,
                subagentModel: options.subagentModel,
                resume: options.resume == true
            )
        }
    }

    public let startId: String
    public let fingerprint: Fingerprint

    /// The machine the start goes to (part of the fingerprint).
    public var deviceId: String { fingerprint.deviceId }

    public init(startId: String, fingerprint: Fingerprint) {
        self.startId = startId
        self.fingerprint = fingerprint
    }

    /// The start a Start or Retry of `fingerprint` sends: the pending one
    /// while the draft (device included) matches, else a freshly minted one.
    public static func resolve(
        pending: DraftStart?,
        fingerprint: Fingerprint,
        mint: () -> String = { UUID().uuidString.lowercased() }
    ) -> DraftStart {
        if let pending, pending.fingerprint == fingerprint {
            return pending
        }
        return DraftStart(startId: mint(), fingerprint: fingerprint)
    }

    /// A landing of this start clears the draft and opens the session only
    /// when the draft is unchanged since its send; a draft the person changed
    /// after the timeout stays exactly as they left it (no navigation).
    public func landsInto(_ current: Fingerprint) -> Bool {
        fingerprint == current
    }
}
