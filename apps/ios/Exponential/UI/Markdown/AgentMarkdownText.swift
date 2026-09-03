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

    /// The steer feed's inline-code palette, generated straight off the shared
    /// design tokens (web `--code-*`, desktop and Android mirror it).
    static let chatCodePalette = MarkdownStyle.Overrides(
        inlineCodeForeground: DesignTokens.Semantic.codeText,
        inlineCodeBackground: DesignTokens.Semantic.codeFill
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
        _displayModel = State(initialValue: Self.model(
            text, context: context, options: options, overrides: overrides
        ))
        _displayedText = State(initialValue: text)
    }

    /// Parsed display models keyed by text + base URL + parse options. The
    /// models are read-only (no focus, no edits), so sharing one between two
    /// bubbles showing the same text is harmless. Bounded: a feed holds at
    /// most `AgentFeed.feedCap` items, and NSCache evicts under pressure.
    private static let cache: NSCache<NSString, IssueEditorModel> = {
        let cache = NSCache<NSString, IssueEditorModel>()
        cache.countLimit = 600
        return cache
    }()

    private static func cacheKey(
        text: String,
        baseURL: URL?,
        options: MarkdownParseOptions,
        overrides: MarkdownStyle.Overrides
    ) -> NSString {
        "\(options.rawValue)|\(overrides.cacheKey)|\(baseURL?.absoluteString ?? "")|\(text)"
            as NSString
    }

    /// Cache hit or a synchronous parse that populates the cache.
    private static func model(
        _ text: String,
        context: AgentMarkdownContext?,
        options: MarkdownParseOptions,
        overrides: MarkdownStyle.Overrides
    ) -> IssueEditorModel {
        let key = cacheKey(
            text: text, baseURL: context?.baseURL, options: options, overrides: overrides
        )
        if let cached = cache.object(forKey: key) { return cached }
        let model = IssueEditorModel()
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
            isReadOnly: true,
            imageMaxHeight: imageMaxHeight,
            hugsContentWidth: hugsWidth
        )
        .frame(maxWidth: hugsWidth ? nil : .infinity, alignment: .leading)
        .onChange(of: text) { _, newText in
            guard displayedText != newText else { return }
            displayedText = newText
            displayModel = Self.model(
                newText, context: context, options: options, overrides: overrides
            )
        }
    }
}
