import ExpCore
import ExpUI
import Foundation

/// The headless-CLI install one-liner, pointed at an account's own instance.
///
/// Lifted out of `GettingStartedCards` (EXP-725) so the first-run wizard's
/// devices step copies the SAME string the checklist's "Set up a server" card
/// does — a phone cannot run it, so both surfaces only ever put it on the
/// pasteboard for the machine that can.
enum ServerInstallSnippet {
    /// Copies the snippet for `accountId`'s instance. Returns false when the
    /// account has no usable instance URL (nothing is copied — the caller
    /// skips its copied-flash).
    @discardableResult
    @MainActor
    static func copy(accountId: String, auth: AuthRepository) -> Bool {
        let instanceUrl = auth.accounts.first { $0.id == accountId }?.instanceUrl
        guard let origin = WebLinks.normalizedBase(instanceUrl) else { return false }
        Platform.copyToPasteboard(AppConstants.serverInstallSnippet(origin: origin))
        return true
    }
}
