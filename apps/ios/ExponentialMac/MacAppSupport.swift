import Foundation

/// The app's Application Support directory (`~/Library/Application Support/
/// Exponential`), created on demand. Shared by the embedded terminal (scratch
/// run dirs) and the preview infra (repos root, trust store). Relocated out of
/// the deleted desktop-agent identity store.
enum MacAppSupport {
    static func dir() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Exponential", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }
}
