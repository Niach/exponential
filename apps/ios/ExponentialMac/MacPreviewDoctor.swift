import Foundation

/// Prerequisite checks per platform, with remediation copy, so the pane can tell
/// the user exactly what to install before a Run. Pure probes via the
/// Foundation.Process path (augmented PATH); no side effects.
enum MacPreviewDoctor {
    struct Check: Identifiable, Sendable {
        let id = UUID()
        let name: String
        let ok: Bool
        let detail: String
        // What to do when `ok` is false.
        let remediation: String?
    }

    struct Report: Sendable {
        let checks: [Check]
        var allOk: Bool { checks.allSatisfy(\.ok) }
        var failures: [Check] { checks.filter { !$0.ok } }
    }

    /// Run the relevant checks for a platform off-main (each probe shells out).
    static func run(for platform: PreviewPlatform) async -> Report {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(returning: Report(checks: probe(platform)))
            }
        }
    }

    private static func probe(_ platform: PreviewPlatform) -> [Check] {
        switch platform {
        case .web: webChecks()
        case .android: androidChecks()
        case .ios: iosChecks()
        }
    }

    // MARK: - Web

    private static func webChecks() -> [Check] {
        [which(
            "node",
            name: "Node.js",
            remediation: "Install Node (brew install node) so the dev server can start."
        )]
    }

    // MARK: - Android

    private static func androidChecks() -> [Check] {
        var checks: [Check] = []
        checks.append(which(
            "adb", name: "Android Platform Tools (adb)",
            remediation: "Install the Android SDK Platform-Tools and ensure adb is on PATH (Android Studio → SDK Manager)."
        ))
        checks.append(which(
            "emulator", name: "Android Emulator",
            remediation: "Install the Emulator package via Android Studio → SDK Manager, or add $ANDROID_HOME/emulator to PATH."
        ))
        // List installed AVDs so the user can confirm the target's `avd` exists.
        let avds = PreviewShell.run("emulator", ["-list-avds"])
        let names = avds.stdout.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        checks.append(Check(
            name: "Android Virtual Devices",
            ok: !names.isEmpty,
            detail: names.isEmpty ? "No AVDs found" : "Found: \(names.joined(separator: ", "))",
            remediation: names.isEmpty
                ? "Create an AVD (Android Studio → Device Manager) matching the target's `avd` name."
                : nil
        ))
        return checks
    }

    // MARK: - iOS

    private static func iosChecks() -> [Check] {
        var checks: [Check] = []
        // Apple Silicon: serve-sim's Simulator embedding assumes an arm64 host.
        let isAppleSilicon: Bool = {
            var sysinfo = utsname()
            uname(&sysinfo)
            let machine = withUnsafeBytes(of: &sysinfo.machine) { raw -> String in
                let ptr = raw.bindMemory(to: CChar.self)
                return String(cString: ptr.baseAddress!)
            }
            return machine.hasPrefix("arm64")
        }()
        checks.append(Check(
            name: "Apple Silicon",
            ok: isAppleSilicon,
            detail: isAppleSilicon ? "arm64" : "Intel host",
            remediation: isAppleSilicon ? nil : "iOS Simulator preview is supported on Apple Silicon Macs."
        ))
        // Xcode command-line tools (xcrun/simctl).
        let xcrun = PreviewShell.run("xcrun", ["--version"])
        checks.append(Check(
            name: "Xcode command-line tools",
            ok: xcrun.ok,
            detail: xcrun.ok ? xcrun.stdout.trimmingCharacters(in: .whitespacesAndNewlines) : "xcrun not found",
            remediation: xcrun.ok ? nil : "Install Xcode + run `xcode-select --install`."
        ))
        checks.append(which(
            "node", name: "Node.js (for serve-sim)",
            remediation: "Install Node (brew install node); serve-sim runs via npx."
        ))
        return checks
    }

    // MARK: - Helpers

    private static func which(_ program: String, name: String, remediation: String) -> Check {
        let result = PreviewShell.run("which", [program])
        let path = result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        let ok = result.ok && !path.isEmpty
        return Check(
            name: name,
            ok: ok,
            detail: ok ? path : "Not found on PATH",
            remediation: ok ? nil : remediation
        )
    }
}
