import Foundation

// `sessions.send`'s wire forms, on their own (Foundation only) so the share
// extension compiles them without the rest of `SessionsApi` (EXP-1635).

// MARK: - sessions.send

/// The composer's runner pickers for a NEW session. Nil fields are omitted
/// from the wire and mean the device's own default; `effort: ""` is an
/// explicit "CLI default". A blank model is never sent (the key is omitted:
/// the server reads absent and blank alike). S-3: no plan mode.
public struct SessionLaunchOptions: Sendable, Equatable {
    /// Contract `codingAgent` (claude/codex).
    public var cli: String?
    /// The agent login profile on the device (`system` = the ambient login);
    /// nil = its last used one.
    public var account: String?
    public var model: String?
    public var effort: String?
    /// nil = the device's own `launch_defaults.computerUse`.
    public var computerUse: Bool?
    /// Claude only.
    public var ultracode: Bool?
    /// The team MCP servers the session gets; nil = none picked.
    public var mcpServerIds: [String]?
    /// #EXP-1661: the team skills the session runs with; nil or empty = none
    /// (never sent).
    public var skillIds: [String]?

    public init(
        cli: String? = nil,
        account: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        computerUse: Bool? = nil,
        ultracode: Bool? = nil,
        mcpServerIds: [String]? = nil,
        skillIds: [String]? = nil
    ) {
        self.cli = cli
        self.account = account
        self.model = model
        self.effort = effort
        self.computerUse = computerUse
        self.ultracode = ultracode
        self.mcpServerIds = mcpServerIds
        self.skillIds = skillIds
    }
}

/// #EXP-1715 (composer-menu.json `teamsPick`): the Assistant's Teams pick
/// a send carries as `scopeTeamIds`: every team (an explicit JSON null) or
/// the picked teams. Absent (nil on the case) = not the Assistant: the key
/// is never sent.
public enum TeamsScope: Sendable, Equatable {
    case allTeams
    case teams([String])

    /// The stored pick (nil = every team) as a scope.
    public init(_ picked: [String]?) {
        self = picked.map(TeamsScope.teams) ?? .allTeams
    }

    public var picked: [String]? {
        if case let .teams(ids) = self { return ids }
        return nil
    }
}

/// The forms of `sessions.send`. `teamId` is the ACTIVE team (phones:
/// the current board's team).
public enum SessionsSendInput: Sendable, Equatable {
    /// A new CHAT root: the composer, Start on an issue (`issueIds`), a
    /// saved prompt run (NEW-2 `promptId`: its body plus `text` = the first
    /// message). Needs `text`, `issueIds` or a `promptId`.
    case newChat(
        teamId: String,
        deviceId: String,
        options: SessionLaunchOptions = SessionLaunchOptions(),
        text: String? = nil,
        attachmentIds: [String]? = nil,
        issueIds: [String]? = nil,
        promptId: String? = nil,
        repositoryId: String? = nil
    )
    /// NEW-2: a message to a team AGENT — into the caller's own conversation
    /// session with it (created on first use; it runs on the agent's own
    /// device, so no runner picks). Needs text or files. EXP-1450:
    /// `issueIds` = the composer's issue chips, forwarded onto it.
    /// #EXP-1715: `scope` = the Assistant's Teams pick (nil = not sent).
    /// #EXP-1793: `mcpServerIds` = the conversation's MCP servers from now
    /// on (an empty list clears; nil = not sent, the pick stands).
    case agent(
        agentId: String, text: String? = nil, attachmentIds: [String]? = nil, issueIds: [String]? = nil,
        scope: TeamsScope? = nil, mcpServerIds: [String]? = nil
    )
    /// VAPP-110: a message to an APP — into the caller's own EDITOR session
    /// with it (created on first use on `deviceId`, else the caller's default
    /// device; an ended one continues). Needs text or files. #EXP-1729:
    /// `account` / `model` = the editor composer's picks, riding the send
    /// that opens or revives the root, with the agent the login belongs to
    /// (`cli`, so the server reads the model in that agent's list; nil =
    /// omitted).
    case app(
        appId: String, deviceId: String?, text: String? = nil, attachmentIds: [String]? = nil,
        cli: String? = nil, account: String? = nil, model: String? = nil
    )
    /// A CHILD session of `parentId` on any device.
    case child(
        parentId: String,
        deviceId: String,
        options: SessionLaunchOptions = SessionLaunchOptions(),
        text: String? = nil
    )
    /// A message into an existing session (resumes it when idle; an ended
    /// chat revives server-side, the same row). Needs text or files.
    /// #EXP-1715: `scope` = the Assistant's Teams pick, into its
    /// conversation root only (nil = not sent).
    case message(sessionId: String, text: String? = nil, attachmentIds: [String]? = nil, scope: TeamsScope? = nil)
    /// EXP-849: the mid-session account SWITCH — `{sessionId, account}`, no
    /// text. The server moves the synced `sessions.account`; the host device
    /// resumes the SAME row on that login between turns.
    case switchAccount(sessionId: String, account: String)
    /// EXP-1252: MOVE the session to another device — `{sessionId, deviceId}`,
    /// no text. The server moves it at once (a queued row re-targets, a live
    /// one ends here and continues on that device).
    case moveDevice(sessionId: String, deviceId: String)
}

// Synthesized-style encodeIfPresent per field: every nil is OMITTED, never
// `null` (the router's fields are `.optional()`, not `.nullable()`), save
// #EXP-1715's `scopeTeamIds` (null = every team).
extension SessionsSendInput: Encodable {
    private enum Keys: String, CodingKey {
        case teamId, sessionId, parentId, deviceId
        case cli, account, model, effort, computerUse, ultracode, mcpServerIds, skillIds
        case text, attachmentIds, issueIds, promptId, repositoryId, agentId, appId, scopeTeamIds
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        // #EXP-1715: every team = an explicit null (the ONE null this form
        // sends); absent = not the Assistant.
        func encodeScope(_ scope: TeamsScope?) throws {
            switch scope {
            case nil: break
            case .allTeams: try c.encodeNil(forKey: .scopeTeamIds)
            case let .teams(ids): try c.encode(ids, forKey: .scopeTeamIds)
            }
        }
        func encodeOptions(_ options: SessionLaunchOptions) throws {
            try c.encodeIfPresent(options.cli, forKey: .cli)
            try c.encodeIfPresent(options.account, forKey: .account)
            try c.encodeIfPresent(options.model.flatMap { $0.isEmpty ? nil : $0 }, forKey: .model)
            try c.encodeIfPresent(options.effort, forKey: .effort)
            try c.encodeIfPresent(options.computerUse, forKey: .computerUse)
            try c.encodeIfPresent(options.ultracode, forKey: .ultracode)
            try c.encodeIfPresent(options.mcpServerIds, forKey: .mcpServerIds)
            // #EXP-1661: only while the pick is non-empty.
            try c.encodeIfPresent(options.skillIds.flatMap { $0.isEmpty ? nil : $0 }, forKey: .skillIds)
        }
        switch self {
        case let .newChat(teamId, deviceId, options, text, attachmentIds, issueIds, promptId, repositoryId):
            try c.encode(teamId, forKey: .teamId)
            try c.encode(deviceId, forKey: .deviceId)
            try encodeOptions(options)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
            try c.encodeIfPresent(issueIds, forKey: .issueIds)
            try c.encodeIfPresent(promptId, forKey: .promptId)
            try c.encodeIfPresent(repositoryId, forKey: .repositoryId)
        case let .agent(agentId, text, attachmentIds, issueIds, scope, mcpServerIds):
            try c.encode(agentId, forKey: .agentId)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
            try c.encodeIfPresent(issueIds, forKey: .issueIds)
            try encodeScope(scope)
            try c.encodeIfPresent(mcpServerIds, forKey: .mcpServerIds)
        case let .app(appId, deviceId, text, attachmentIds, cli, account, model):
            try c.encode(appId, forKey: .appId)
            try c.encodeIfPresent(deviceId, forKey: .deviceId)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
            try c.encodeIfPresent(cli, forKey: .cli)
            try c.encodeIfPresent(account, forKey: .account)
            try c.encodeIfPresent(model.flatMap { $0.isEmpty ? nil : $0 }, forKey: .model)
        case let .child(parentId, deviceId, options, text):
            try c.encode(parentId, forKey: .parentId)
            try c.encode(deviceId, forKey: .deviceId)
            try encodeOptions(options)
            try c.encodeIfPresent(text, forKey: .text)
        case let .message(sessionId, text, attachmentIds, scope):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(attachmentIds, forKey: .attachmentIds)
            try encodeScope(scope)
        case let .switchAccount(sessionId, account):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(account, forKey: .account)
        case let .moveDevice(sessionId, deviceId):
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(deviceId, forKey: .deviceId)
        }
    }
}

/// `sessions.send`'s answer: the session the call started or messaged, and
/// the `messages` row its text became (nil for a start without text).
public struct SessionsSendResult: Decodable, Sendable, Equatable {
    public let sessionId: String
    public let messageId: String?

    public init(sessionId: String, messageId: String? = nil) {
        self.sessionId = sessionId
        self.messageId = messageId
    }
}
