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
// eUI `ChoiceTiles` primitive), and the `custom` free-text row. Answered =
// closed: the picks stay highlighted, everything inert.

/// The person's in-progress answer to one rich card: the picked labels and
/// the typed `custom` text per question (a question card's own form uses the
/// key `""`), and the inline surface's live model.
@MainActor
@Observable
final class PlanCardDraft {
    var picks: [String: [String]] = [:]
    var custom: [String: String] = [:]
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

/// One question block (a plan's question, or a question card's tiles):
/// `editable` = the card still waits on this reader; `picked` = the labels
/// shown picked (the draft's, or the stamped answers once closed).
struct PlanCardQuestionBlock: View {
    let questionId: String
    /// nil = no prompt line (a question card's prompt is the card body).
    let prompt: String?
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
        VStack(alignment: .leading, spacing: 6) {
            VStack(alignment: .leading, spacing: 2) {
                if let prompt, !prompt.isEmpty {
                    Text(prompt)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.white)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Text(multi ? PlanCard.Copy.chooseAny : PlanCard.Copy.chooseOne)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            switch PlanCard.optionRows(options) {
            case .list:
                VStack(alignment: .leading, spacing: 4) {
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

    /// `copy.custom` = the free-text row: a glass field while answerable;
    /// closed, a typed answer (a pick that is no option) reads as a picked row.
    @ViewBuilder
    private var customRow: some View {
        if editable {
            GlassTextField(
                PlanCard.Copy.custom,
                text: Binding(get: { customText }, set: { onType($0) }),
                lines: 1...4,
                verticalPadding: 6,
                accessibilityIdentifier: PlanCard.TestIds.custom
            ) {
                EmptyView()
            } trailing: {
                EmptyView()
            }
            .font(.caption)
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

/// THE option row of the rich card (the question card's rows: the label
/// over its description, a check glyph when picked; a multiple choice wears
/// its checkbox). Inert once the card is answered.
struct PlanCardOptionRow: View {
    let option: TreeQuestionOption
    let picked: Bool
    let multi: Bool
    let editable: Bool
    let onTap: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
        let row = HStack(alignment: .center, spacing: 8) {
            if multi {
                AppIcon(picked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: 13)
                    .foregroundStyle(.white.opacity(picked ? TextOpacity.primary : TextOpacity.tertiary))
            }
            VStack(alignment: .leading, spacing: 2) {
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
                AppIcon(AppIcons.uiCheck, size: 12)
                    .foregroundStyle(DesignTokens.Palette.primary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .frame(minHeight: 32)
        .contentShape(Rectangle())
        .background(picked ? GlassTokens.fillActive : GlassTokens.fillRow, in: shape)
        .overlay(
            shape.stroke(picked ? GlassTokens.strokeActive : GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .opacity(editable || picked ? 1 : 0.6)
        if editable {
            Button(action: onTap) { row }
                .buttonStyle(.plain)
                .accessibilityAddTraits(picked ? [.isSelected] : [])
        } else {
            row
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(picked ? [.isSelected] : [])
        }
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
        let shape = RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
        VStack(alignment: .leading, spacing: 6) {
            Text(PlanCard.Copy.surface)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            content
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GlassTokens.fillRow, in: shape)
        .overlay(shape.stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
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
