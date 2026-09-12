import ExpCore
import ExpUI
import SwiftUI

/// EXP-849 — an agent's brand mark, never a bare `Image("agent-\(id)")`.
///
/// The brand marks are HAND-maintained assets (the icon generator owns only
/// the `lucide-*` imagesets), so the catalog carries exactly one per CONTRACT
/// `codingAgent` and nothing else. An id from outside that set — a historical
/// row whose agent the contract dropped, a future agent, an external ACP
/// binary — interpolated into an asset name resolves to NOTHING and renders a
/// silent blank pill. Those fall back to the neutral coding-assistant concept
/// glyph, so a row always shows something.
///
/// Adding an agent to the contract means adding its `agent-<id>.imageset`
/// beside `agent-claude` / `agent-codex`; until that lands it draws the
/// neutral glyph instead of nothing.
enum AgentBrandMark {
    static func image(_ agent: String) -> Image? {
        if DomainContract.codingAgentValues.contains(agent) {
            return Image("agent-\(agent)")
        }
        // A registry glyph ships as a TEMPLATE imageset: the tint comes from
        // the caller's foreground style, exactly as `AppIcon` renders it.
        guard let asset = AppIcons.assetName(AppIcons.codingAssistant) else { return nil }
        return Image(asset).renderingMode(.template)
    }
}
