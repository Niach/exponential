import CryptoKit
import ExpCore
import Foundation

// MARK: - Platform

/// The run-target platforms. Mirrors the generated
/// `DomainContract.platformValues` (web/android/ios/command) as a typed enum;
/// unknown values from a newer repo file are dropped (the target is simply not
/// runnable here). `web`/`android`/`ios` are preview backends; `command` is a
/// generic host-side process launched from the play menu into a terminal-dock
/// tab (masterplan §4c) — it has no embed surface.
enum PreviewPlatform: String, Codable, CaseIterable, Sendable {
    case web
    case android
    case ios
    case command

    var displayName: String {
        switch self {
        case .web: "Web"
        case .android: "Android"
        case .ios: "iOS"
        case .command: "Command"
        }
    }

    var sfSymbol: String {
        switch self {
        case .web: "globe"
        case .android: "candybarphone"
        case .ios: "iphone"
        case .command: "terminal"
        }
    }

    init?(wire: String) {
        switch wire {
        case DomainContract.platformWeb: self = .web
        case DomainContract.platformAndroid: self = .android
        case DomainContract.platformIos: self = .ios
        case DomainContract.platformCommand: self = .command
        default: return nil
        }
    }
}

// MARK: - Run target (parsed from .exponential/config.json)

/// One named run target from the committed repo file. The `platform`
/// discriminator selects the backend; the rest is the per-platform build/run
/// shell config (canonical, executed ONLY from the cloned working tree behind
/// the trust gate — never from the synced DB mirror). Fields are optional so a
/// partial/forward-compatible config still decodes; the backend validates what
/// it needs at run time.
struct RunTarget: Identifiable, Sendable, Equatable {
    let id: String
    let name: String
    let platform: PreviewPlatform
    let rootDir: String?
    let env: [String: String]

    // web
    let setup: String?
    let run: String?
    let url: String?
    let readyPath: String?
    let injectWidget: Bool?

    // android
    let build: String?
    let apk: String?
    let applicationId: String?
    let activity: String?
    let avd: String?

    // ios
    let scheme: String?
    let workspace: String?
    let simulator: String?
    let bundleId: String?

    // command (generic host-side process — masterplan §4c)
    /// Program + arguments, spawned directly (no shell). Min 1 element.
    let argv: [String]?
    /// Repo-relative working directory ('..' rejected at parse time).
    let cwd: String?

    /// The ordered, canonical set of shell commands this target would execute —
    /// the input to the trust hash. Only commands (never URLs / ids) so a benign
    /// metadata edit doesn't force a re-prompt, while any change to what RUNS does.
    var commandSet: [String] {
        switch platform {
        case .web:
            return [setup, run].compactMap { $0 }
        case .android:
            return [setup, build].compactMap { $0 }
        case .ios:
            // iOS build/run is driven by xcodebuild/serve-sim from scheme/workspace,
            // not free-form shell — include those identifiers so a scheme swap
            // re-prompts.
            return [scheme, workspace, bundleId].compactMap { $0 }
        case .command:
            // Fold argv AND cwd into the hash so the trust gate re-prompts when
            // either changes (the repo file is agent-editable — the prompt is
            // the security boundary; masterplan §4c.2).
            var set = argv ?? []
            if let cwd, !cwd.isEmpty { set.append("cwd:\(cwd)") }
            return set
        }
    }
}

// MARK: - Decoding the repo file

/// `{ version: 1, targets: [...] }`. Decoded leniently: a malformed target is
/// skipped rather than failing the whole file.
struct PreviewConfigFile: Sendable {
    let version: Int
    let targets: [RunTarget]
}

enum MacPreviewConfig {
    static let fileRelativePath = ".exponential/config.json"

    /// The repos root: the preview reads a project's checkout from
    /// `{reposRoot}/{owner}/{repo}` on the default branch.
    static func reposRoot() -> URL {
        MacAppSupport.dir().appendingPathComponent("repos", isDirectory: true)
    }

    /// The main checkout for `owner/repo` (where `.exponential/config.json` lives).
    static func repoWorkingTree(forRepo ownerRepo: String) -> URL? {
        let parts = ownerRepo.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2, !parts[0].isEmpty, !parts[1].isEmpty else { return nil }
        return reposRoot()
            .appendingPathComponent(parts[0], isDirectory: true)
            .appendingPathComponent(parts[1], isDirectory: true)
    }

    /// Resolve + parse `.exponential/config.json` from the cloned working tree.
    /// Returns nil when the repo isn't cloned yet or the file is absent/invalid.
    static func load(forRepo ownerRepo: String) -> PreviewConfigFile? {
        guard let tree = repoWorkingTree(forRepo: ownerRepo) else { return nil }
        let fileURL = tree.appendingPathComponent(fileRelativePath)
        guard let data = try? Data(contentsOf: fileURL) else { return nil }
        return parse(data)
    }

    static func parse(_ data: Data) -> PreviewConfigFile? {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        let version = (root["version"] as? Int) ?? 1
        let rawTargets = (root["targets"] as? [[String: Any]]) ?? []
        let targets = rawTargets.compactMap(decodeTarget)
        return PreviewConfigFile(version: version, targets: targets)
    }

    private static func decodeTarget(_ raw: [String: Any]) -> RunTarget? {
        guard let id = raw["id"] as? String, !id.isEmpty,
              let name = raw["name"] as? String, !name.isEmpty,
              let platformWire = raw["platform"] as? String,
              let platform = PreviewPlatform(wire: platformWire) else { return nil }
        // Reject a traversal rootDir defensively (the server sanitizes too).
        let rootDir = raw["rootDir"] as? String
        if let rootDir, rootDir.contains("..") { return nil }
        // command targets: argv is required (min 1, no shell); cwd is
        // repo-relative with '..' rejected (contract: commandTargetSchema).
        let argv = raw["argv"] as? [String]
        let cwd = raw["cwd"] as? String
        if let cwd, cwd.contains("..") { return nil }
        if platform == .command, (argv ?? []).isEmpty { return nil }
        var env = (raw["env"] as? [String: String]) ?? [:]
        // Strip dangerous env overrides (server does this too; belt-and-braces).
        for key in ["PATH", "LD_PRELOAD", "DYLD_INSERT_LIBRARIES", "DYLD_LIBRARY_PATH", "DYLD_FRAMEWORK_PATH"] {
            env.removeValue(forKey: key)
        }
        return RunTarget(
            id: id,
            name: name,
            platform: platform,
            rootDir: rootDir,
            env: env,
            setup: raw["setup"] as? String,
            run: raw["run"] as? String,
            url: raw["url"] as? String,
            readyPath: raw["readyPath"] as? String,
            injectWidget: raw["injectWidget"] as? Bool,
            build: raw["build"] as? String,
            apk: raw["apk"] as? String,
            applicationId: raw["applicationId"] as? String,
            activity: raw["activity"] as? String,
            avd: raw["avd"] as? String,
            scheme: raw["scheme"] as? String,
            workspace: raw["workspace"] as? String,
            simulator: raw["simulator"] as? String,
            bundleId: raw["bundleId"] as? String,
            argv: argv,
            cwd: cwd
        )
    }

    // MARK: - DB mirror (ProjectEntity.previewConfig JSON text)

    /// Decode the display-only mirror stored on the synced project row. Used for
    /// the target picker before/without a clone and for feedback routing.
    static func parseMirror(_ json: String?) -> ProjectPreviewMirror? {
        guard let json, let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(ProjectPreviewMirror.self, from: data)
    }
}

// MARK: - Mirror types (decoded from the synced project row)

/// The safe display metadata Electric syncs on `projects.preview_config`:
/// the target list (id/name/platform) + the feedback routing target. NEVER
/// carries build/run commands — those come only from the repo file.
struct ProjectPreviewMirror: Codable, Sendable, Equatable {
    struct Target: Codable, Sendable, Equatable, Identifiable {
        let id: String
        let name: String
        let platform: String
    }

    let targets: [Target]
    let feedbackProjectId: String?
}

// MARK: - Trust gate

/// Per-repo approval of the command set the preview would run. The synced mirror
/// is never executed; the repo file's commands are. The first Run is gated
/// behind a one-time "Trust preview commands?" prompt; we re-prompt whenever the
/// approved command set changes (a malicious or unexpected synced command swap
/// can't auto-run), while a benign metadata edit does NOT re-prompt.
@MainActor
enum MacPreviewTrust {
    private struct Store: Codable { var approved: [String: String] = [:] } // repo -> sha256 hex

    private static func storeURL() -> URL {
        MacAppSupport.dir().appendingPathComponent("preview-trust.json")
    }

    private static func load() -> Store {
        guard let data = try? Data(contentsOf: storeURL()),
              let store = try? JSONDecoder().decode(Store.self, from: data) else { return Store() }
        return store
    }

    private static func save(_ store: Store) {
        guard let data = try? JSONEncoder().encode(store) else { return }
        try? data.write(to: storeURL())
    }

    /// Hash the resolved command set for a repo's targets. The repo identity is
    /// folded in so two repos with identical commands stay independently trusted.
    static func hash(repo: String, targets: [RunTarget]) -> String {
        var lines = ["repo:\(repo)"]
        for target in targets.sorted(by: { $0.id < $1.id }) {
            lines.append("target:\(target.id):\(target.platform.rawValue)")
            for command in target.commandSet { lines.append("cmd:\(command)") }
        }
        let joined = lines.joined(separator: "\n")
        let digest = SHA256.hash(data: Data(joined.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    /// Is the current command set already approved for this repo?
    static func isTrusted(repo: String, targets: [RunTarget]) -> Bool {
        load().approved[repo] == hash(repo: repo, targets: targets)
    }

    /// Record approval of the current command set for this repo.
    static func approve(repo: String, targets: [RunTarget]) {
        var store = load()
        store.approved[repo] = hash(repo: repo, targets: targets)
        save(store)
    }

    /// Forget a repo's approval (e.g. after a clear-trust action).
    static func revoke(repo: String) {
        var store = load()
        store.approved.removeValue(forKey: repo)
        save(store)
    }
}
