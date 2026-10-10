import ExpCore
import ExpUI
import SwiftUI

/// What an agent-markdown render needs to fetch the images it embeds: the
/// instance base URL the relative `/api/attachments/{id}` paths resolve
/// against, plus the credentials the attachment loader authenticates with
/// (EXP-440). Built once per screen, exactly like `CommentThreadView` does it.
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
}

/// Read-only GFM render of agent-authored prose (plan bodies, question
/// prompts, narration, steered messages) through the SAME block stack as
/// comment bodies (EXP-249) — claude writes markdown, and a plan flattened
/// into one Text was unreadable. EXP-686 moved it out of `AgentSessionView`
/// (internal, not private) so the runs list can render a close-out summary
/// as real markdown too.
struct AgentMarkdownText: View {
    let text: String
    /// nil renders text-only: image blocks have nothing to fetch with.
    let context: AgentMarkdownContext?
    /// Display-only parse deviations — safe here, nothing serializes back.
    let options: MarkdownParseOptions
    let imageMaxHeight: CGFloat?
    /// Chat bubbles hug their text; full-width prose does not.
    let hugsWidth: Bool
    /// EXP-698: display-only palette deviations. The chat feed is the ONE
    /// surface that tints inline code (`Semantic.codeText` on `codeFill`) —
    /// comment and issue renders keep the neutral interchange look.
    let overrides: MarkdownStyle.Overrides

    @State private var displayModel: IssueEditorModel
    @State private var displayedText: String?
    /// EXP-787: the metrics are baked into the attributed string at parse time,
    /// so a text-size change has to re-parse — the `@State` model outlives the
    /// `init` that built it. The render cache keys on the RESOLVED metrics, so
    /// stepping back to a previous size is a cache hit.
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    /// EXP-1188: optional so a render outside the app shell (previews) just
    /// keeps UIKit's default link open.
    @Environment(AppDependencies.self) private var deps: AppDependencies?

    /// The steer feed's inline-code palette, generated straight off the shared
    /// design tokens (web `--code-*`, desktop and Android mirror it) — plus
    /// (EXP-787) the transcript's own measure: agent prose reads at
    /// `Transcript.bodySize` on a `Transcript.bodyLineHeight` line, while every
    /// editable surface keeps the interchange body font. Both numbers are the
    /// DEFAULT-size values; `MarkdownStyle` scales them through `UIFontMetrics`
    /// at render time, so the transcript keeps Dynamic Type.
    static let chatCodePalette = MarkdownStyle.Overrides(
        inlineCodeForeground: DesignTokens.Semantic.codeText,
        inlineCodeBackground: DesignTokens.Semantic.codeFill,
        bodySize: DesignTokens.Transcript.bodySize,
        lineHeight: DesignTokens.Transcript.bodyLineHeight
    )

    /// The Guide's report sections: the transcript's measure, but NEUTRAL
    /// code spans (the interchange style) — no chat palette (polish pin ×4).
    static let guideReport = MarkdownStyle.Overrides(
        bodySize: DesignTokens.Transcript.bodySize,
        lineHeight: DesignTokens.Transcript.bodyLineHeight
    )

    init(
        text: String,
        context: AgentMarkdownContext? = nil,
        options: MarkdownParseOptions = [],
        imageMaxHeight: CGFloat? = 280,
        hugsWidth: Bool = false,
        overrides: MarkdownStyle.Overrides = AgentMarkdownText.chatCodePalette
    ) {
        self.text = text
        self.context = context
        self.options = options
        self.imageMaxHeight = imageMaxHeight
        self.hugsWidth = hugsWidth
        self.overrides = overrides
        // EXP-582: a LazyVStack drops a row's @State the moment it scrolls
        // off, so every re-realized bubble used to start EMPTY, re-run cmark
        // in `.task` and then grow to its real height — the layout churn
        // behind the scroll lag on long histories (and a feed replay made
        // it N times over). Parse SYNCHRONOUSLY on first creation (EXP-580,
        // the EXP-70 failure mode again) and cache the result, so a
        // re-realized row renders at full height on the first pass and a
        // cache hit parses nothing.
        // EXP-1238: the row OWNS its model and adopts the cached parse's
        // blocks — a shared model cannot be reloaded in place (two rows
        // showing one text would follow each other's updates), and in-place
        // is the point: see `adoptDisplayBlocks`.
        _displayModel = State(initialValue: Self.rowModel(
            text, context: context, options: options, overrides: overrides
        ))
        _displayedText = State(initialValue: text)
    }

    /// The row's own display model, holding the cached prototype's blocks.
    private static func rowModel(
        _ text: String,
        context: AgentMarkdownContext?,
        options: MarkdownParseOptions,
        overrides: MarkdownStyle.Overrides
    ) -> IssueEditorModel {
        let model = IssueEditorModel()
        configureDisplay(model, context: context)
        model.adoptDisplayBlocks(from: prototype(
            text, context: context, options: options, overrides: overrides
        ))
        return model
    }

    /// Display-only wiring shared by the cached prototypes and the row models:
    /// read-only chips resolved against the run's team through the same memo
    /// the issue editors use (`bareIssueRefs` is what makes a narrated
    /// `EXP-758` chip alongside `#EXP-758`, EXP-760), and EXP-824's attachment
    /// resolver so a narrated `[clip.mp4](/api/attachments/{id})` gets the
    /// same player. The raw text is untouched, nothing here ever serializes.
    private static func configureDisplay(_ model: IssueEditorModel, context: AgentMarkdownContext?) {
        guard let refs = context?.issueRefs else { return }
        let accountId = context?.accountId ?? ""
        model.attachmentResolver = AttachmentInfoCache.resolver(db: refs.db, accountId: accountId)
        let scope = IssueRefLookup.Scope.team(id: refs.teamId)
        let db = refs.db
        model.isDisplayOnly = true
        model.bareIssueRefs = refs.bareRefs
        model.issueRefResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: db, accountId: accountId)?
                .issueId
        }
        model.issueRefTitleResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: db, accountId: accountId)?
                .title
        }
        model.issueRefStatusResolver = { identifier in
            IssueRefChipCache.statusInfo(
                identifier, scope: scope, db: db, accountId: accountId)
        }
    }

    /// Parsed display PROTOTYPES keyed by text + base URL + parse options:
    /// read-only models whose blocks the rows adopt (EXP-1238), never
    /// rendered themselves, so sharing one between two bubbles showing the
    /// same text is harmless. Bounded by `countLimit`
    /// below and by NSCache's own eviction under pressure — EXP-783 uncapped
    /// the feed itself, so the count limit here is the only bound that
    /// matters (and the session view renders a window of it anyway).
    private static let cache: NSCache<NSString, IssueEditorModel> = {
        let cache = NSCache<NSString, IssueEditorModel>()
        cache.countLimit = 600
        return cache
    }()

    private static func cacheKey(
        text: String,
        baseURL: URL?,
        options: MarkdownParseOptions,
        overrides: MarkdownStyle.Overrides,
        // The chip pass bakes resolved ids/titles into the model, so two
        // accounts or two teams must never share one (EXP-760).
        refsKey: String
    ) -> NSString {
        "\(options.rawValue)|\(overrides.cacheKey)|\(baseURL?.absoluteString ?? "")|\(refsKey)|\(text)"
            as NSString
    }

    /// Cache hit or a synchronous parse that populates the cache.
    private static func prototype(
        _ text: String,
        context: AgentMarkdownContext?,
        options: MarkdownParseOptions,
        overrides: MarkdownStyle.Overrides
    ) -> IssueEditorModel {
        let refs = context?.issueRefs
        let accountId = context?.accountId ?? ""
        let key = cacheKey(
            text: text, baseURL: context?.baseURL, options: options, overrides: overrides,
            refsKey: refs.map { "\(accountId)/\($0.teamId)/\($0.bareRefs)" } ?? ""
        )
        if let cached = cache.object(forKey: key) { return cached }
        let model = IssueEditorModel()
        configureDisplay(model, context: context)
        model.load(
            markdown: text, baseURL: context?.baseURL, options: options, overrides: overrides
        )
        cache.setObject(model, forKey: key)
        return model
    }

    var body: some View {
        MarkdownEditor(
            model: displayModel,
            placeholder: "",
            baseURL: context?.baseURL,
            accountId: context?.accountId ?? "",
            httpClient: context?.httpClient,
            onIssueRefTap: context?.issueRefs?.onOpen,
            isReadOnly: true,
            imageMaxHeight: imageMaxHeight,
            hugsContentWidth: hugsWidth
        )
        .environment(\.markdownLinkHandler, linkHandler)
        .frame(maxWidth: hugsWidth ? nil : .infinity, alignment: .leading)
        // EXP-1238: a streamed fragment or a text-size change reloads the
        // row's model IN PLACE (block ids kept), so the text view SwiftUI
        // already holds re-applies its content and re-measures — never a
        // fresh UITextView per fragment.
        .onChange(of: text) { _, newText in
            guard displayedText != newText else { return }
            displayedText = newText
            displayModel.adoptDisplayBlocks(from: Self.prototype(
                newText, context: context, options: options, overrides: overrides
            ))
        }
        .onChange(of: dynamicTypeSize) { _, _ in
            displayModel.adoptDisplayBlocks(from: Self.prototype(
                text, context: context, options: options, overrides: overrides
            ))
        }
    }

    // MARK: - Link taps (EXP-1188)

    /// A tapped link classified against the account's instance
    /// (`AppLink.classify`, fixture-locked ×4): a run or issue on this
    /// instance opens in-app, a placeholder link does nothing, the rest keep
    /// UIKit's own open.
    private var linkHandler: MarkdownLinkHandler? {
        guard let deps else { return nil }
        let accountId = context?.accountId.nilIfEmpty ?? deps.auth.activeAccountId ?? ""
        let base = context?.baseURL ?? deps.auth.instanceBaseURL(forAccountId: accountId)
        let origin = base?.absoluteString ?? ""
        return { url in Self.linkAction(url, origin: origin, accountId: accountId, deps: deps) }
    }

    private static func linkAction(
        _ url: URL, origin: String, accountId: String, deps: AppDependencies
    ) -> (() -> Void)? {
        switch AppLink.classify(url.absoluteString, origin: origin) {
        case .external:
            // EXP-1188: a web source opens in the in-app Safari sheet, like
            // Android's Custom Tab; mailto keeps the system handler.
            guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
                return nil
            }
            return { deps.deepLinkBus.openExternal(url) }
        case .ignore:
            return {}
        case let .session(_, sessionId):
            let userId = deps.auth.accounts.first(where: { $0.id == accountId })?.userId
            return { deps.deepLinkBus.navigateToSession(sessionId, userId: userId) }
        case let .issue(teamSlug, _, identifier):
            return {
                if let issueId = IssueRefLookup.resolve(
                    identifier: identifier, teamSlug: teamSlug, db: deps.db, accountId: accountId
                ) {
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                } else {
                    // Not synced / not visible: the in-app Safari sheet —
                    // never UIApplication.open, the app is entitled for the
                    // link and would re-open itself.
                    deps.deepLinkBus.openExternal(url)
                }
            }
        case .app:
            // The one other page the app renders natively (EXP-825); every
            // other instance page goes to the Safari sheet for the same
            // entitlement reason as above.
            if case let .agent(teamSlug) = WebLinks.parse(url) {
                return { deps.deepLinkBus.navigateToAgent(teamSlug: teamSlug, accountId: accountId) }
            }
            return { deps.deepLinkBus.openExternal(url) }
        }
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
