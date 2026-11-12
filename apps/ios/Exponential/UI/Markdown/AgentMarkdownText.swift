import ExpCore
import ExpUI
import GRDB
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

/// What an agent-markdown render needs to fetch the images it embeds: the
/// instance base URL the relative `/api/attachments/{id}` paths resolve
/// against, plus the credentials the attachment loader authenticates with
/// (EXP-440). Built once per screen, exactly like `CommentChatView` does it.
struct AgentMarkdownContext {
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    /// EXP-760: set by the steering screens so resolved issue identifiers chip.
    var issueRefs: AgentIssueRefContext?
}

/// EXP-760 — what the steering feed needs to chip issue identifiers: the run's
/// TEAM (a batch / action / chat run has no issue and no board, so the team is
/// the only scope there is), the store to resolve against, and where a tap
/// goes. Provided only by the session screens: this is the ONE surface that
/// also chips BARE `EXP-758` tokens, because agents narrate them without a
/// `#`. Descriptions and comments keep the `#IDENTIFIER` contract.
struct AgentIssueRefContext {
    let teamId: String
    let db: DatabaseManager
    let onOpen: (String) -> Void
    /// Off for an uploaded markdown file: only `#IDENT` chips there, as on
    /// the web; agent narration keeps the bare form.
    var bareRefs = true
    /// The team's members, so a chat's `@email` renders as the comments'
    /// round name pill (`ChatMentionMembers`); an unresolved address stays
    /// text. Empty = no pills.
    var mentionMembers: [MentionMember] = []

    /// Part of the prototype cache key and the row's re-adopt trigger: the
    /// pill pass bakes the resolved NAMES into the model.
    var mentionKey: String { ChatMentionMembers.fingerprint(mentionMembers) }
}

/// The `@` vocabulary of a chat surface, off the two synced tables (both
/// whole and small), memoized per account × team for a few seconds, like
/// `IssueRefChipCache`: a feed re-renders every row on each emission, and
/// they all share one read. #PERF-5: read from a view body, so memory only:
/// none until the members load off main.
@MainActor
enum ChatMentionMembers {
    private struct Key: Hashable, Sendable {
        let accountId: String
        let teamId: String
    }

    private static let memo = StoreMemo<Key, [MentionMember]>(capacity: 64)

    static func members(teamId: String, db: DatabaseManager, accountId: String) -> [MentionMember] {
        memo.peek(Key(accountId: accountId, teamId: teamId)) {
            guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
            let members = (try? await pool.read { db -> [MentionMember] in
                let memberIds = try TeamMemberEntity
                    .filter(Column("team_id") == teamId)
                    .fetchAll(db)
                    .map(\.userId)
                return try UserEntity.filter(keys: memberIds).fetchAll(db)
                    .map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
            }) ?? []
            return members.sorted { $0.email < $1.email }
        } ?? []
    }

    nonisolated static func fingerprint(_ members: [MentionMember]) -> String {
        members.isEmpty ? "" : members.map { "\($0.email)=\($0.name)" }.joined(separator: ",")
    }
}

/// #VAPP-258: how a read-only text renders (`catalog/host.json` markdown
/// options): `chat` = the transcript rhythm (`density: chat`), `hardBreaks`
/// = every newline breaks the line (agent output is written for a terminal,
/// EXP-440), `autolink` = bare `http(s)://` urls link, `imageMaxHeight` = a
/// block picture's height cap (nil = none: the width at its aspect).
struct AppMarkdownStyle: Equatable {
    var chat: Bool
    var hardBreaks: Bool
    var autolink: Bool
    var imageMaxHeight: CGFloat?
    /// EXP-1280 (plan-card.json `card`): the plan card's inline picture
    /// frame (`pictureRadius` + a `pictureStroke` hairline), painted by the
    /// `Markdown/image` part of the plan-card theme (`AppMarkdownHost`).
    var planCardPictures = false

    /// A chat's block picture cap (pt); documents have none.
    static let chatImageMaxHeight: CGFloat = 240

    /// Agent narration, chat messages, steered prompts.
    static let chat = AppMarkdownStyle(chat: true, hardBreaks: true, autolink: true, imageMaxHeight: chatImageMaxHeight)
    /// A question / plan card's prompt in a chat: the chat rhythm, soft breaks.
    static let prompt = AppMarkdownStyle(chat: true, hardBreaks: false, autolink: true, imageMaxHeight: chatImageMaxHeight)
    /// Comments, plan pages, results, uploaded files.
    static let document = AppMarkdownStyle(chat: false, hardBreaks: false, autolink: false, imageMaxHeight: nil)

    /// Bare urls link (a plan page, an agent-written card).
    var autolinked: AppMarkdownStyle {
        var s = self
        s.autolink = true
        return s
    }

    /// A plan card's pictures (`planCardPictures`).
    var planCard: AppMarkdownStyle {
        var s = self
        s.planCardPictures = true
        return s
    }

    func imageCap(_ cap: CGFloat?) -> AppMarkdownStyle {
        var s = self
        s.imageMaxHeight = cap
        return s
    }
}

/// Read-only Markdown: issue comments, the agent transcript and chat,
/// plan pages, results and file previews. #VAPP-258: ONE implementation
/// with genUI's catalog `Markdown` (ExponentialUI's `MarkdownText`, the
/// `Markdown/*` recipes of the app's theme); this view supplies the host
/// hooks (`AppMarkdownHost`). Editors keep their own renderer
/// (`MarkdownEditor`).
struct AgentMarkdownText: View {
    let text: String
    /// nil renders without the account: no pictures, no chips.
    let context: AgentMarkdownContext?
    let style: AppMarkdownStyle
    /// `streaming: true`: `text` is a growing prefix (a reply being written).
    let streaming: Bool
    /// Chat bubbles hug their text; full-width prose does not.
    let hugsWidth: Bool

    @Environment(AppDependencies.self) private var deps: AppDependencies?
    /// EXP-1493: a host that opens a picture its own way (the plan lightbox).
    @Environment(\.markdownImageTapHandler) private var onImageTap
    @State private var picture: PictureViewerSet?

    init(
        text: String,
        context: AgentMarkdownContext? = nil,
        style: AppMarkdownStyle = .chat,
        streaming: Bool = false,
        hugsWidth: Bool = false
    ) {
        self.text = text
        self.context = context
        self.style = style
        self.streaming = streaming
        self.hugsWidth = hugsWidth
    }

    var body: some View {
        #if canImport(ExponentialUI)
        if let host = AppMarkdownHost.host(context, deps: deps, planCard: style.planCardPictures) {
            let source = host.prepare(text, autolink: style.autolink)
            MarkdownText(
                source,
                context: host.model,
                options: MarkdownOptions(
                    hardBreaks: style.hardBreaks, chat: style.chat, selectable: true,
                    streaming: streaming, imageMaxHeight: style.imageMaxHeight
                ),
                hugsWidth: hugsWidth,
                // #PERF-5: the host's chips load off main; a landed answer
                // re-parses the text.
                revision: "\(host.revision)|\(StoreMemoLoads.shared.tick)"
            )
            .frame(maxWidth: hugsWidth ? nil : .infinity, alignment: .leading)
            .onAppear { register(host, source) }
            .onChange(of: source) { _, next in register(host, next) }
            .pictureViewer($picture)
        }
        #endif
    }

    #if canImport(ExponentialUI)
    /// A press on one of this text's pictures opens THE picture viewer (or
    /// the screen's own handler).
    private func register(_ host: AppMarkdownHost, _ source: String) {
        guard source.contains("![") else { return }
        let custom = onImageTap
        host.registerPictures(in: source) { src in
            if let custom {
                custom(src)
            } else {
                let id = AttachmentLinks.attachmentId(fromUrl: src, baseURL: context?.baseURL) ?? src
                picture = PictureViewerSet(items: [PictureViewerItem(id: id, src: src, label: "")])
            }
        }
    }
    #endif
}
