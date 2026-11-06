import ExpCore
import ExpUI
import SwiftUI

private typealias Measures = Chats.Shell.ChatHeader

/// #EXP-1688 (chats.json `shell.chatHeader`): THE chat header on a phone (web
/// `MobileDetailHeader` + `PhoneHeaderTitle`, Android `ChatHeader`): one row,
/// ALWAYS leading: the ghost back, the title block taking the width that is
/// left, the trailing slot. It rides the first row of the header band
/// (`workHeaderBand`); the screen hides its navigation bar, whose edge swipe
/// stays armed by `bareBackButton`.
struct ChatHeader<Title: View, Trailing: View>: View {
    @ViewBuilder var title: () -> Title
    @ViewBuilder var trailing: () -> Trailing

    @Environment(\.dismiss) private var dismiss
    @Environment(\.detailColumnRoot) private var columnRoot

    var body: some View {
        HStack(spacing: CGFloat(Measures.gap)) {
            // The id UI tests find Back by (`bareBackButton`'s too). #NEW-12:
            // none at the iPad detail column's root (its list is beside it).
            if !columnRoot {
                ChatHeaderGhostButton(icon: AppIcons.uiBack, label: "Back", identifier: "BackButton") {
                    dismiss()
                }
            }
            title()
                .frame(maxWidth: .infinity, alignment: .leading)
            trailing()
        }
        .padding(.horizontal, CGFloat(Measures.paddingX))
        .frame(maxWidth: .infinity, minHeight: CGFloat(Measures.row), maxHeight: CGFloat(Measures.row), alignment: .leading)
    }
}

extension ChatHeader where Title == EmptyView, Trailing == EmptyView {
    /// The row while there is no chat to title (first load, not found).
    init() {
        self.init(title: { EmptyView() }, trailing: { EmptyView() })
    }
}

/// A ghost button's face: the glyph at the secondary text colour, no fill and
/// no hairline (the filled circle is the page header's).
struct ChatHeaderGhostGlyph: View {
    let icon: String
    var opacity = TextOpacity.secondary

    var body: some View {
        AppIcon(icon, size: CGFloat(Measures.glyph), weight: .medium)
            .foregroundStyle(.white.opacity(opacity))
            .frame(width: CGFloat(Measures.button), height: CGFloat(Measures.button))
            .contentShape(Circle())
    }
}

struct ChatHeaderGhostButton: View {
    let icon: String
    let label: String
    var identifier: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ChatHeaderGhostGlyph(icon: icon)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier ?? "")
    }
}

/// EXP-1373 (chats.json, the chat screen on phones): the header's leading
/// block — the lead DISC (the agent's icon in its colour, else the run mark:
/// amber-badged while it needs you, the working mark while busy) and the
/// title over ONE subtitle line (the state word, the device;
/// an agent chat: `Agent · <model>`).
struct ChatHeaderTitle: View {
    enum Tone { case muted, working, needsYou }

    let title: String
    var subtitle: String?
    var subtitleTone: Tone = .muted
    var isPrivate = false
    var agentIcon: String?
    var agentColor: String?
    /// #EXP-1448: the viewer's Assistant wears the brand mark (chats.json),
    /// never its stored icon.
    var assistant = false
    var cli: String?
    /// The root's PR states: the lead's PR glyph while nothing works.
    var prStates: [String] = []
    var mark: ThreadRowMark = .waiting

    /// EXP-1552: THE chat lead ×4 (web `ChatLeadDisc`, Android
    /// `chatLeadOf`, the Chats list's `ChatLeadMark`): the mark first, then
    /// the root's PR glyph, else the bare chat glyph.
    private var lead: Chats.ChatLead {
        switch mark {
        case .needsYou: .needsYou
        case .working: .working
        case .waiting:
            switch SessionTree.threadRowLeadPr(prStates: prStates) {
            case .open: .open
            case .merged: .merged
            case nil: .waiting
            }
        }
    }

    var body: some View {
        HStack(spacing: CGFloat(Measures.discGap)) {
            ZStack {
                Circle().fill(DesignTokens.Palette.muted)
                if assistant {
                    ExpLogoMark(size: CGFloat(Measures.discGlyph), color: .white)
                } else if let agentIcon {
                    AppIcon(agentIcon, size: CGFloat(Measures.discGlyph), weight: .medium)
                        .foregroundStyle(Color(hex: agentColor) ?? .white)
                } else {
                    ChatLeadMark(lead: lead, cli: cli, size: CGFloat(Measures.discGlyph))
                }
            }
            .frame(width: CGFloat(Measures.disc), height: CGFloat(Measures.disc))
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(title)
                        .font(.system(size: CGFloat(Measures.title), weight: .semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("thread-title")
                    if isPrivate {
                        AppIcon(AppIcons.uiPrivate, size: 11)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .accessibilityLabel("Private")
                            .accessibilityIdentifier("thread-private")
                    }
                }
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.system(size: CGFloat(Measures.subtitle)))
                        .foregroundStyle(subtitleColor)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .accessibilityIdentifier("thread-subtitle")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// Web `ChatHeaderTitle`: needs input = amber, working = the foreground,
    /// otherwise the muted default.
    private var subtitleColor: Color {
        switch subtitleTone {
        case .needsYou: DesignTokens.Semantic.yellow
        case .working: .white
        case .muted: .white.opacity(TextOpacity.secondary)
        }
    }

    private static func live(_ rows: ThreadRows) -> [SessionEntity] {
        rows.sessions.filter { $0.status != DomainContract.sessionStatusEnded }
    }

    /// The subtitle's parts, ` · `-joined.
    static func subtitle(rows: ThreadRows) -> String {
        if rows.isAgent {
            return Chats.agentChatSubtitle(modelLabel: rows.root.model.map(LaunchVocabulary.modelLabel))
        }
        let live = live(rows)
        let state: String? = if live.contains(where: \.needsInput) {
            ThreadRowWord.needsInput.text
        } else if live.contains(where: \.busy) {
            Chats.Copy.Preview.working
        } else {
            nil
        }
        return [state, rows.rootDevice?.label].compactMap { $0 }.joined(separator: " · ")
    }

    static func tone(rows: ThreadRows) -> Tone {
        guard !rows.isAgent else { return .muted }
        let live = live(rows)
        if live.contains(where: \.needsInput) { return .needsYou }
        return live.contains(where: \.busy) ? .working : .muted
    }
}
