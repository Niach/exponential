import ExpCore
import ExpUI
import SwiftUI
import UIKit
#if canImport(ExponentialUI)
import ExponentialUI
#endif

// EXP-1280 custom plan mode: THE rich plan / question card's form parts
// (fixtures/plan-card.json ×4: web `session-thread-view.tsx` + `ask-card.tsx`,
// desktop `thread_screen.rs`, Android `ThreadParts.kt`). `ThreadAskCard` hands
// them to `QuestionCard` (the AskCard header + the markdown body + the
// decision rows stay THE card): the inline surface, then one block per
// question = the prompt (strong), `chooseOne` / `chooseAny`, the options as
// THE option rows (text only) or as picture TILES (`GlassChoiceTiles`, the
// eUI `ChoiceTiles` primitive), and the `custom` free-text field. Answered =
// closed: the picks highlighted, the rest inert. Every number comes from
// `PlanCard.Card` / `PlanCard.Tiles` (the ONE look ×4).

/// The person's in-progress answer to one rich card: the picked labels and
/// the typed `custom` text per question (a question card's own form uses the
/// key `""`), and the inline surface's live model.
@MainActor
@Observable
final class PlanCardDraft {
    var picks: [String: [String]] = [:]
    var custom: [String: String] = [:]
    /// The plan's feedback field (sent with any decision).
    var feedback = ""
    @ObservationIgnored let surface = PlanCardSurfaceHandle()

    /// A tap on an option: a single choice picks it (the typed text yields),
    /// a multiple one toggles it.
    func toggle(_ questionId: String, label: String, multi: Bool) {
        var current = picks[questionId] ?? []
        if multi {
            if let index = current.firstIndex(of: label) { current.remove(at: index) } else { current.append(label) }
        } else {
            current = current == [label] ? [] : [label]
            custom[questionId] = ""
        }
        picks[questionId] = current
    }

    /// Typing the `custom` answer: a single choice drops its pick.
    func type(_ questionId: String, text: String, multi: Bool) {
        custom[questionId] = text
        if !multi, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { picks[questionId] = [] }
    }

    /// The labels one question answers with (picks, then the typed text).
    func answer(_ questionId: String) -> [String] {
        var labels = picks[questionId] ?? []
        let typed = (custom[questionId] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if !typed.isEmpty { labels.append(typed) }
        return labels
    }

    /// A plan's answers by question id (unanswered questions left out).
    func planAnswers(_ questions: [ThreadPlanQuestion]) -> [String: [String]] {
        var answers: [String: [String]] = [:]
        for question in questions {
            let labels = answer(question.id)
            if !labels.isEmpty { answers[question.id] = labels }
        }
        return answers
    }
}

/// One question block (a plan's question, or a question card's options):
/// `editable` = the card still waits on this reader; `picked` = the labels
/// shown picked (the draft's, or the stamped answers once closed). The
/// prompt `xs` semibold, the choose caption `2xs` muted `blockInnerGap`
/// under it, the options `optionsGap` under that, then THE `custom` field.
struct PlanCardQuestionBlock: View {
    let questionId: String
    /// nil = no prompt line (a question card's prompt is the card body).
    let prompt: String?
    /// The choose caption (a plan's question: always; a question card: only
    /// "Choose any" while a multi waits). nil = none.
    let caption: String?
    let multi: Bool
    let custom: Bool
    let options: [TreeQuestionOption]
    let picked: [String]
    let editable: Bool
    @Binding var customText: String
    let onToggle: (String) -> Void
    let onType: (String) -> Void

    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        VStack(alignment: .leading, spacing: PlanCard.Card.optionsGap) {
            if (prompt?.isEmpty == false) || caption != nil {
                VStack(alignment: .leading, spacing: PlanCard.Card.blockInnerGap) {
                    if let prompt, !prompt.isEmpty {
                        Text(prompt)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let caption {
                        Text(caption)
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
            }
            switch PlanCard.optionRows(options) {
            case .list:
                VStack(alignment: .leading, spacing: PlanCard.Tiles.labelGap) {
                    ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                        PlanCardOptionRow(
                            option: option,
                            picked: picked.contains(option.label),
                            multi: multi,
                            editable: editable
                        ) { onToggle(option.label) }
                        .accessibilityIdentifier("\(PlanCard.TestIds.option)-\(index + 1)")
                    }
                }
            case .tiles:
                GlassChoiceTiles(
                    options.enumerated().map { index, option in
                        GlassChoiceTile(
                            label: option.label,
                            description: option.description,
                            hasPicture: option.attachmentId != nil,
                            identifier: "\(PlanCard.TestIds.tile)-\(index + 1)"
                        )
                    },
                    selected: Set(options.indices.filter { picked.contains(options[$0].label) }),
                    multiple: multi,
                    enabled: editable,
                    columns: sizeClass == .compact ? PlanCard.Tiles.phoneColumns : PlanCard.Tiles.columns,
                    onToggle: { onToggle(options[$0].label) }
                ) { index in
                    if let attachmentId = options[index].attachmentId {
                        PlanCardPicture(attachmentId: attachmentId)
                    }
                }
            }
            if custom {
                customRow
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanCard.TestIds.question)
    }

    /// `copy.custom` = THE free-text field: single line, a row's height;
    /// closed, a typed answer (a pick that is no option) reads as a picked row.
    @ViewBuilder
    private var customRow: some View {
        if editable {
            PlanCardField(
                placeholder: PlanCard.Copy.custom,
                text: Binding(get: { customText }, set: { onType($0) }),
                multiline: false,
                identifier: PlanCard.TestIds.custom
            )
        } else if let typed = picked.first(where: { label in !options.contains { $0.label == label } }) {
            PlanCardOptionRow(
                option: TreeQuestionOption(label: typed),
                picked: true,
                multi: multi,
                editable: false
            ) {}
            .accessibilityIdentifier(PlanCard.TestIds.custom)
        }
    }
}

/// THE card's plain field (plan-card.json): the `custom` answer (single
/// line, `rowMinHeight`) and the plan's feedback (multi-line,
/// `fieldMinHeight`), `xs` type.
struct PlanCardField: View {
    let placeholder: String
    @Binding var text: String
    let multiline: Bool
    let identifier: String

    var body: some View {
        GlassTextField(
            placeholder,
            text: $text,
            lines: multiline ? 1...5 : nil,
            verticalPadding: PlanCard.Card.optionsGap,
            minHeight: multiline ? PlanCard.Card.fieldMinHeight : PlanCard.Card.rowMinHeight,
            accessibilityIdentifier: identifier
        )
        .font(.caption)
    }
}

/// THE option row of the rich card. Open: the label over its description in
/// a glass row, a single choice's pick = a TRAILING check, a multiple
/// choice's = the LEADING selected / unselected glyph (no check). Closed: no
/// fill, a leading emerald check on the picks, the rest muted at
/// `inertOpacity`.
struct PlanCardOptionRow: View {
    let option: TreeQuestionOption
    let picked: Bool
    let multi: Bool
    let editable: Bool
    let onTap: () -> Void

    private static let glyph: CGFloat = 14

    var body: some View {
        if editable {
            Button(action: onTap) { openRow }
                .buttonStyle(.plain)
                .accessibilityAddTraits(picked ? [.isSelected] : [])
        } else {
            closedRow
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(picked ? [.isSelected] : [])
        }
    }

    private var openRow: some View {
        let shape = RoundedRectangle(cornerRadius: PlanCard.Card.rowRadius)
        return HStack(alignment: .center, spacing: 6) {
            if multi {
                AppIcon(picked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: Self.glyph)
                    .foregroundStyle(.white.opacity(picked ? TextOpacity.primary : TextOpacity.tertiary))
            }
            VStack(alignment: .leading, spacing: PlanCard.Card.blockInnerGap) {
                Text(option.label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.leading)
                if let description = option.description, !description.isEmpty {
                    Text(description)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if picked, !multi {
                AppIcon(AppIcons.uiCheck, size: Self.glyph)
                    .foregroundStyle(.white)
            }
        }
        .padding(.horizontal, PlanCard.Card.blockGap)
        .padding(.vertical, PlanCard.Card.optionsGap)
        .frame(minHeight: PlanCard.Card.rowMinHeight)
        .contentShape(Rectangle())
        .background(picked ? GlassTokens.fillActive : GlassTokens.fillRow, in: shape)
        .overlay(
            shape.stroke(picked ? GlassTokens.strokeActive : GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
    }

    private var closedShape: RoundedRectangle { RoundedRectangle(cornerRadius: PlanCard.Card.rowRadius) }

    private var closedRow: some View {
        HStack(alignment: .center, spacing: 6) {
            ZStack {
                if picked {
                    AppIcon(AppIcons.uiCheck, size: Self.glyph)
                        .foregroundStyle(DesignTokens.Semantic.green)
                }
            }
            .frame(width: Self.glyph, height: Self.glyph)
            Text(option.label)
                .font(.caption)
                .foregroundStyle(.white.opacity(picked ? TextOpacity.primary : PlanCard.Card.inertOpacity))
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, PlanCard.Card.blockGap)
        .padding(.vertical, PlanCard.Card.optionsGap)
        .frame(minHeight: PlanCard.Card.rowMinHeight)
        // The picks keep the open row's picked fill + stroke; the rest none.
        .background(picked ? GlassTokens.fillActive : Color.clear, in: closedShape)
        .overlay(closedShape.stroke(picked ? GlassTokens.strokeActive : Color.clear, lineWidth: GlassTokens.hairline))
    }
}

/// An option's picture through the instance (the attachment loader the
/// markdown images and result tiles share), aspect-filled into its tile.
struct PlanCardPicture: View {
    let attachmentId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        ZStack {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .clipped()
        .accessibilityHidden(true)
        .task(id: attachmentId) {
            let loader = AttachmentImageLoader(
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load("/api/attachments/\(attachmentId)")
        }
    }
}

// MARK: - The inline surface

/// The card's hold on its inline surface: the live renderer model (when the
/// app links the Exponential UI painter) whose data model is snapshotted into
/// `surfaceData` when the person answers.
@MainActor
final class PlanCardSurfaceHandle {
    #if canImport(ExponentialUI)
    var model: SurfaceModel?
    #endif

    /// The surface's data model now (nil = no live surface).
    func snapshot() -> CardJSON? {
        #if canImport(ExponentialUI)
        guard let model,
              let data = model.data.json.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]),
              let value = CardJSON(any: object),
              value.objectValue != nil
        else { return nil }
        return value
        #else
        return nil
        #endif
    }
}

/// The agent-authored Exponential UI core surface, full card width in its own
/// frame (`plan-card-surface`): the real renderer with a card host (no
/// transport: actions go nowhere, host functions are denied, so only the
/// core's `set` runs; media = `/api/attachments/*` through the instance with
/// the account's auth, else https). Without the painter linked: the frame
/// reading `copy.surface`.
struct PlanCardSurfaceView: View {
    let surface: CardSurface
    let handle: PlanCardSurfaceHandle
    /// Answered = closed: the surface paints its data but takes no input.
    let editable: Bool

    var body: some View {
        // The `copy.surface` label ABOVE its frame (web `PlanCardSurface`).
        let shape = RoundedRectangle(cornerRadius: PlanCard.Card.surfaceRadius)
        VStack(alignment: .leading, spacing: PlanCard.Card.surfaceLabelGap) {
            Text(PlanCard.Copy.surface)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            content
                .padding(PlanCard.Card.surfacePadding)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(GlassTokens.fillRow, in: shape)
                .clipShape(shape)
                .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: PlanCard.Card.pictureStroke))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanCard.TestIds.surface)
    }

    @ViewBuilder
    private var content: some View {
        #if canImport(ExponentialUI)
        PlanCardLiveSurface(surface: surface, handle: handle)
            .allowsHitTesting(editable)
        #else
        EmptyView()
        #endif
    }
}

#if canImport(ExponentialUI)
/// The live renderer: one `SurfaceModel` per card, fed the NestedNode root and
/// the initial data model once.
private struct PlanCardLiveSurface: View {
    let surface: CardSurface
    let handle: PlanCardSurfaceHandle

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var model: SurfaceModel?
    @State private var failed = false

    var body: some View {
        Group {
            if let model {
                ExponentialSurface(model: model)
            } else if failed {
                EmptyView()
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .task(id: surface.rootJSON) { build() }
    }

    private func build() {
        let host = PlanCardSurfaceHost(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
        )
        do {
            let next = try SurfaceModel(
                id: "plan-card-\(abs(surface.rootJSON.hashValue))",
                options: SurfaceOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark),
                host: host
            )
            try next.setNested(json: surface.rootJSON)
            for (key, value) in surface.data?.objectValue ?? [:] {
                let pointer = "/" + key.replacingOccurrences(of: "~", with: "~0").replacingOccurrences(of: "/", with: "~1")
                next.setData(path: pointer, value: JSONValue.parse(value.compactString))
            }
            model = next
            handle.model = next
        } catch {
            failed = true
        }
    }
}

/// The card's host plugin: no transport, no host functions; media and links
/// resolve against the instance, attachments load with the account's auth.
@MainActor
private final class PlanCardSurfaceHost: HostPlugin {
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient

    init(baseURL: URL?, accountId: String, httpClient: HTTPClient) {
        self.baseURL = baseURL
        self.accountId = accountId
        self.httpClient = httpClient
    }

    private var base: String? {
        baseURL.map { $0.absoluteString.hasSuffix("/") ? String($0.absoluteString.dropLast()) : $0.absoluteString }
    }

    var mediaOptions: MediaOptions? {
        // https everywhere; the instance's own scheme (a LAN self-host may be
        // plain http) only for its attachments.
        MediaOptions(baseUrl: base, schemes: ["https"] + (baseURL?.scheme == "http" ? ["http"] : []))
    }

    var urlPolicy: UrlPolicy? { UrlPolicy(schemes: ["https"], baseUrl: base) }

    func mediaRequest(_ src: String) -> URLRequest? {
        guard let base else { return nil }
        let attachments = "\(base)/api/attachments/"
        if src.hasPrefix("/api/attachments/") || src.hasPrefix(attachments) {
            let absolute = src.hasPrefix("/") ? base + src : src
            guard let url = URL(string: absolute) else { return nil }
            return httpClient.request(url, accountId: accountId, contentType: nil)
        }
        // Anything else: an absolute https url through the media policy.
        guard src.lowercased().hasPrefix("https://") else { return nil }
        return policyMediaRequest(src, options: MediaOptions(schemes: ["https"]))
    }
}
#endif
