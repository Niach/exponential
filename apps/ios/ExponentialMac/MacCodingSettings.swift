import ExpCore
import Foundation

/// Persistent desktop coding settings (masterplan §4b): the `claude` CLI path,
/// the repos/worktree root, the branch prefix, and the user's personal API key.
/// Stored as JSON in the app-support dir.
///
/// TODO(Phase 6): move `personalApiKey` into the macOS Keychain instead of this
/// plaintext JSON file (it is 0o600 for now, but the real fix is the Keychain).
@MainActor
@Observable
final class MacCodingSettings {
    var claudePath: String
    var reposRoot: String
    var branchPrefix: String
    /// The full raw personal API key — written into each worktree's `.mcp.json`.
    var personalApiKey: String?
    /// The key record id (to revoke) + its visible prefix (to display).
    var personalApiKeyId: String?
    var personalApiKeyStart: String?

    private init(
        claudePath: String,
        reposRoot: String,
        branchPrefix: String,
        personalApiKey: String?,
        personalApiKeyId: String?,
        personalApiKeyStart: String?
    ) {
        self.claudePath = claudePath
        self.reposRoot = reposRoot
        self.branchPrefix = branchPrefix
        self.personalApiKey = personalApiKey
        self.personalApiKeyId = personalApiKeyId
        self.personalApiKeyStart = personalApiKeyStart
    }

    static var defaultReposRoot: String { NSHomeDirectory() + "/Exponential/repos" }

    /// The repos root as a filesystem URL (tilde-expanded).
    var reposRootURL: URL {
        URL(fileURLWithPath: (reposRoot as NSString).expandingTildeInPath, isDirectory: true)
    }

    var hasPersonalKey: Bool { !(personalApiKey ?? "").isEmpty }

    // MARK: - Persistence (JSON in MacAppSupport.dir())

    private struct Stored: Codable {
        var claudePath: String
        var reposRoot: String
        var branchPrefix: String
        var personalApiKey: String?
        var personalApiKeyId: String?
        var personalApiKeyStart: String?
    }

    private static var fileURL: URL {
        MacAppSupport.dir().appendingPathComponent("coding-settings.json")
    }

    static func load() -> MacCodingSettings {
        if let data = try? Data(contentsOf: fileURL),
           let s = try? JSONDecoder().decode(Stored.self, from: data) {
            return MacCodingSettings(
                claudePath: s.claudePath.isEmpty ? "claude" : s.claudePath,
                reposRoot: s.reposRoot.isEmpty ? defaultReposRoot : s.reposRoot,
                branchPrefix: s.branchPrefix.isEmpty ? "exp/" : s.branchPrefix,
                personalApiKey: s.personalApiKey,
                personalApiKeyId: s.personalApiKeyId,
                personalApiKeyStart: s.personalApiKeyStart
            )
        }
        return MacCodingSettings(
            claudePath: "claude",
            reposRoot: defaultReposRoot,
            branchPrefix: "exp/",
            personalApiKey: nil,
            personalApiKeyId: nil,
            personalApiKeyStart: nil
        )
    }

    func save() {
        let s = Stored(
            claudePath: claudePath,
            reposRoot: reposRoot,
            branchPrefix: branchPrefix,
            personalApiKey: personalApiKey,
            personalApiKeyId: personalApiKeyId,
            personalApiKeyStart: personalApiKeyStart
        )
        guard let data = try? JSONEncoder().encode(s) else { return }
        try? data.write(to: Self.fileURL)
        // The key is secret-ish — tighten perms (Keychain is the real fix; see TODO).
        try? FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: Self.fileURL.path)
    }

    /// Store a freshly-minted key + revoke id + display prefix, and persist.
    func setPersonalKey(_ minted: MintedApiKey) {
        personalApiKey = minted.key
        personalApiKeyId = minted.id
        personalApiKeyStart = minted.start
        save()
    }

    /// Forget the local key (after a server-side revoke).
    func clearPersonalKey() {
        personalApiKey = nil
        personalApiKeyId = nil
        personalApiKeyStart = nil
        save()
    }
}
