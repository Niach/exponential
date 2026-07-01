import AppKit
import Foundation
import WebKit

/// Carries a main-isolated callback across a `@Sendable` boundary whose producer
/// is documented to fire on the main actor (the agent terminal runner). The
/// hop-back uses `MainActor.assumeIsolated` at the call site.
private final class MainActorBox<T>: @unchecked Sendable {
    let value: @MainActor (T) -> Void
    init(_ value: @escaping @MainActor (T) -> Void) { self.value = value }
}

// MARK: - Backend contract

/// What a backend needs to build + run the selected target. The working tree is
/// the cloned repo checkout; `rootDir` is resolved relative to it.
struct PreviewRunContext: Sendable {
    let target: RunTarget
    let repo: String
    let accountId: String
    let workingTree: URL

    /// Absolute working directory for the target (workingTree/rootDir).
    var resolvedRootDir: URL {
        guard let rootDir = target.rootDir, !rootDir.isEmpty else { return workingTree }
        return workingTree.appendingPathComponent(rootDir, isDirectory: true)
    }
}

/// One platform backend. Owns its child processes + the embed surface; reports
/// phase transitions and hands the surface back to the controller. All callbacks
/// are delivered on the main actor (the controller marshals there).
@MainActor
protocol PreviewBackend: AnyObject {
    /// Kick off build → run → embed. `onPhase` reports each transition;
    /// `onSurface` delivers the NSView to mount (nil clears it).
    func start(
        context: PreviewRunContext,
        onPhase: @escaping @MainActor (PreviewPhase) -> Void,
        onSurface: @escaping @MainActor (NSView?) -> Void
    )

    /// Idempotent ordered teardown (detach embed → graceful kill → hard kill →
    /// free ports). Safe to call when never started.
    func stop()

    /// A clean (un-annotated) frame of the live preview, in image-pixel space,
    /// for the annotate overlay. nil if unavailable.
    func captureFrame() async -> CGImage?
}

// MARK: - Shell helper (silent probes + visible logs)

/// Off-main shell execution for silent probes (boot polls, adb, simctl, lsof).
/// Visible build/run logs go through `MacTerminalRunner.shared` (the dock)
/// instead; this is the `Foundation.Process` path, with the same augmented PATH
/// so `emulator`/`adb`/`xcrun`/`node` resolve under a GUI launch.
enum PreviewShell {
    /// Common SDK locations layered onto the inherited PATH.
    static func augmentedEnvironment(_ overrides: [String: String] = [:]) -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        let home = NSHomeDirectory()
        let extras = [
            "/opt/homebrew/bin", "/usr/local/bin", "\(home)/.local/bin",
            // Android SDK default install + cmdline-tools.
            "\(home)/Library/Android/sdk/platform-tools",
            "\(home)/Library/Android/sdk/emulator",
            "\(home)/Library/Android/sdk/cmdline-tools/latest/bin",
        ]
        let base = env["PATH"].map { $0 + ":" } ?? ""
        env["PATH"] = base + extras.joined(separator: ":")
        if env["ANDROID_HOME"] == nil { env["ANDROID_HOME"] = "\(home)/Library/Android/sdk" }
        for (k, v) in overrides { env[k] = v }
        return env
    }

    struct Result: Sendable {
        let code: Int32
        let stdout: String
        let stderr: String
        var ok: Bool { code == 0 }
    }

    /// Run a program (resolved via the augmented PATH) to completion, capturing
    /// output. Blocking — call from a detached Task / background queue.
    @discardableResult
    static func run(
        _ program: String,
        _ args: [String],
        cwd: URL? = nil,
        env: [String: String] = [:]
    ) -> Result {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [program] + args
        process.environment = augmentedEnvironment(env)
        if let cwd { process.currentDirectoryURL = cwd }
        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err
        do { try process.run() } catch {
            return Result(code: -1, stdout: "", stderr: "launch failed: \(error.localizedDescription)")
        }
        let outData = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return Result(
            code: process.terminationStatus,
            stdout: String(data: outData, encoding: .utf8) ?? "",
            stderr: String(data: errData, encoding: .utf8) ?? ""
        )
    }

    /// Capture binary stdout (e.g. `adb exec-out screencap -p`). nil on failure.
    static func runBinary(_ program: String, _ args: [String], env: [String: String] = [:]) -> Data? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [program] + args
        process.environment = augmentedEnvironment(env)
        let out = Pipe()
        process.standardOutput = out
        process.standardError = Pipe()
        do { try process.run() } catch { return nil }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return process.terminationStatus == 0 ? data : nil
    }

    /// Free a well-known localhost port by SIGTERM-ing its listener (port hygiene
    /// on teardown + reclaim before respawn). Best-effort.
    static func freePort(_ port: Int) {
        let lsof = run("lsof", ["-ti", "tcp:\(port)", "-sTCP:LISTEN"])
        let pids = lsof.stdout.split(whereSeparator: \.isNewline).compactMap { Int32($0.trimmingCharacters(in: .whitespaces)) }
        for pid in pids { kill(pid, SIGTERM) }
    }
}

/// Long-running visible process started via the agent terminal dock so the user
/// can watch the build/dev-server output. Tracked so teardown can terminate it.
/// (The dock hosts ONE terminal at a time; the preview reuses the per-run window
/// path by passing a unique runId.)
@MainActor
final class PreviewLogRun {
    let runId: String
    private(set) var finished = false

    init(runId: String) { self.runId = runId }

    /// Run `program args` (with the augmented PATH baked into the wrapper env) in
    /// a visible terminal window so the dev-server / build log is watchable.
    func start(
        program: String,
        args: [String],
        cwd: URL,
        env: [String: String],
        title: String,
        onExit: @escaping @MainActor (Int32) -> Void
    ) {
        // The runner's onDone is @Sendable but is always invoked on the main
        // actor. Box the main-isolated callback so the @Sendable closure can hop
        // back and run it there.
        let box = MainActorBox { [weak self] (code: Int32) in
            self?.finished = true
            onExit(code)
        }
        MacTerminalRunner.shared.run(
            runId: runId,
            program: program,
            argv: args,
            env: PreviewShell.augmentedEnvironment(env),
            cwd: cwd.path,
            prompt: "",
            interactive: false,
            issueIdentifier: title
        ) { code, _ in
            DispatchQueue.main.async { MainActor.assumeIsolated { box.value(code) } }
        }
    }

    func stop() {
        guard !finished else { return }
        MacTerminalRunner.shared.terminate(runId: runId)
        finished = true
    }
}

// MARK: - Web backend (WKWebView @ local URL)

/// Builds (setup) + runs the dev server, health-polls `url+readyPath`, then loads
/// it in a WKWebView. The page's own JS feedback widget runs in-page.
@MainActor
final class WebPreviewBackend: NSObject, PreviewBackend {
    private var webView: WKWebView?
    private var devServer: PreviewLogRun?
    private var pollTask: Task<Void, Never>?
    private var port: Int?

    func start(
        context: PreviewRunContext,
        onPhase: @escaping @MainActor (PreviewPhase) -> Void,
        onSurface: @escaping @MainActor (NSView?) -> Void
    ) {
        let target = context.target
        guard let urlString = target.url, let url = URL(string: urlString) else {
            onPhase(.error("Web target is missing a `url`."))
            return
        }
        port = url.port
        let readyURL = URL(string: (target.readyPath.map { urlString + $0 }) ?? urlString) ?? url

        // localhost-only guard: refuse a non-loopback preview URL.
        guard Self.isLoopback(url) else {
            onPhase(.error("Preview URL must be localhost."))
            return
        }

        let webView = WKWebView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
        webView.setValue(false, forKey: "drawsBackground")
        self.webView = webView
        onSurface(webView)

        // 1. setup (optional) → 2. dev server (visible log) → 3. health-poll → load.
        onPhase(.setup)
        let setup = target.setup
        let runCmd = target.run
        let root = context.resolvedRootDir
        let env = target.env

        Task { @MainActor in
            if let setup, !setup.isEmpty {
                let result = await Self.background { PreviewShell.bash(setup, cwd: root, env: env) }
                if !result.ok {
                    onPhase(.error("Setup failed:\n\(result.stderr.isEmpty ? result.stdout : result.stderr)"))
                    return
                }
            }
            guard let runCmd, !runCmd.isEmpty else {
                onPhase(.error("Web target is missing a `run` command."))
                return
            }
            onPhase(.building)
            // Free a stale dev server on the same port before respawning.
            if let p = self.port { PreviewShell.freePort(p) }
            let server = PreviewLogRun(runId: "preview-web-\(UUID().uuidString)")
            self.devServer = server
            server.start(
                program: "bash", args: ["-lc", runCmd], cwd: root, env: env,
                title: "Preview — \(target.name)"
            ) { code in
                if code != 0 { onPhase(.error("Dev server exited (code \(code)).")) }
            }
            onPhase(.booting)
            self.pollTask = Task { @MainActor in
                let ready = await Self.pollHealth(readyURL)
                if Task.isCancelled { return }
                guard ready else {
                    onPhase(.error("Dev server didn't become ready at \(readyURL.absoluteString)."))
                    return
                }
                webView.load(URLRequest(url: url))
                onPhase(.running)
            }
        }
    }

    func stop() {
        pollTask?.cancel(); pollTask = nil
        devServer?.stop(); devServer = nil
        webView?.stopLoading()
        webView = nil
        if let port { PreviewShell.freePort(port) }
        port = nil
    }

    func captureFrame() async -> CGImage? {
        guard let webView else { return nil }
        return await withCheckedContinuation { continuation in
            let config = WKSnapshotConfiguration()
            config.afterScreenUpdates = true
            webView.takeSnapshot(with: config) { image, _ in
                continuation.resume(returning: image?.cgImage(forProposedRect: nil, context: nil, hints: nil))
            }
        }
    }

    // GET url+readyPath every ~300ms until 2xx/3xx, 60s budget. Off the main run
    // loop via URLSession (async), but the timing lives here on the main actor.
    private static func pollHealth(_ url: URL) async -> Bool {
        let deadline = Date().addingTimeInterval(60)
        var request = URLRequest(url: url)
        request.timeoutInterval = 2
        request.cachePolicy = .reloadIgnoringLocalCacheData
        while Date() < deadline {
            if Task.isCancelled { return false }
            if let (_, response) = try? await URLSession.shared.data(for: request),
               let http = response as? HTTPURLResponse, (200...399).contains(http.statusCode) {
                return true
            }
            try? await Task.sleep(for: .milliseconds(300))
        }
        return false
    }

    private static func isLoopback(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
    }

    /// Run a blocking closure off-main and await it.
    private static func background<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async { continuation.resume(returning: work()) }
        }
    }
}

// MARK: - Android backend (emulator + ScreenCaptureKit)

/// Boots the SDK emulator, installs + launches the APK, and embeds the emulator
/// window via ScreenCaptureKit. FIRST CUT: the emulator runs in its own window
/// (run-alongside) and the pane shows a "running" status + a Capture action that
/// uses `adb exec-out screencap -p` for the annotation frame; the SCStream-into-
/// NSView mirroring lands in the on-Mac iteration (TODO below). Annotation frames
/// are pixel-exact via adb regardless.
@MainActor
final class AndroidPreviewBackend: NSObject, PreviewBackend {
    private var emulatorRun: PreviewLogRun?
    private var buildRun: PreviewLogRun?
    private var avd: String?
    // A placeholder embed surface (status card) until SCStream mirroring lands.
    private var container: NSView?
    private var statusLabel: NSTextField?

    func start(
        context: PreviewRunContext,
        onPhase: @escaping @MainActor (PreviewPhase) -> Void,
        onSurface: @escaping @MainActor (NSView?) -> Void
    ) {
        let target = context.target
        guard let avd = target.avd, !avd.isEmpty else {
            onPhase(.error("Android target is missing an `avd`."))
            return
        }
        guard let applicationId = target.applicationId, !applicationId.isEmpty else {
            onPhase(.error("Android target is missing an `applicationId`."))
            return
        }
        self.avd = avd

        let container = NSView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
        container.wantsLayer = true
        container.layer?.backgroundColor = NSColor.black.cgColor
        let label = NSTextField(labelWithString: "Starting Android emulator…")
        label.textColor = .white
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: container.leadingAnchor, constant: 16),
            label.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -16),
        ])
        self.container = container
        self.statusLabel = label
        onSurface(container)

        let build = target.build
        let apk = target.apk
        let activity = target.activity ?? ".MainActivity"
        let root = context.resolvedRootDir
        let env = target.env

        Task { @MainActor in
            // 1. Boot the emulator (visible log; -no-snapshot-save keeps state
            //    clean; -grpc 8554 for the gRPC stream the SCStream fallback uses).
            onPhase(.booting)
            PreviewShell.freePort(8554)
            let emu = PreviewLogRun(runId: "preview-android-emu-\(UUID().uuidString)")
            self.emulatorRun = emu
            emu.start(
                program: "emulator",
                args: ["@\(avd)", "-grpc", "8554", "-no-snapshot-save", "-no-boot-anim"],
                cwd: root, env: env, title: "Emulator — \(avd)"
            ) { _ in }

            let booted = await Self.background { Self.waitForBoot(timeout: 180) }
            if !booted { onPhase(.error("Emulator didn't finish booting.")); return }
            self.setStatus("Building…")

            // 2. Build the APK (optional — skip when `apk` already points at a
            //    prebuilt artifact and there's no build command).
            if let build, !build.isEmpty {
                onPhase(.building)
                let result = await Self.background { PreviewShell.bash(build, cwd: root, env: env) }
                if !result.ok {
                    onPhase(.error("Build failed:\n\(result.stderr.isEmpty ? result.stdout : result.stderr)"))
                    return
                }
            }

            // 3. Install + launch.
            onPhase(.installing)
            self.setStatus("Installing…")
            guard let apk, !apk.isEmpty else {
                onPhase(.error("Android target is missing an `apk` path.")); return
            }
            let apkPath = root.appendingPathComponent(apk).path
            let install = await Self.background { PreviewShell.run("adb", ["install", "-r", apkPath]) }
            if !install.ok {
                onPhase(.error("adb install failed:\n\(install.stderr.isEmpty ? install.stdout : install.stderr)"))
                return
            }
            onPhase(.launching)
            self.setStatus("Launching…")
            let component = "\(applicationId)/\(activity)"
            _ = await Self.background {
                PreviewShell.run("adb", ["shell", "am", "start", "-n", component])
            }
            self.setStatus("Running in the emulator window.\nDraw on the captured frame with Annotate.")
            // TODO (on-Mac): replace the status card with a ScreenCaptureKit
            // SCStream filtered to the emulator window, rendered into an
            // AVSampleBufferDisplayLayer on `container`. Run-alongside until then;
            // annotation frames are already pixel-exact via adb screencap.
            onPhase(.running)
        }
    }

    func stop() {
        // Graceful: ask the running emulator to quit, then stop the visible runs.
        DispatchQueue.global(qos: .userInitiated).async {
            _ = PreviewShell.run("adb", ["emu", "kill"])
        }
        emulatorRun?.stop(); emulatorRun = nil
        buildRun?.stop(); buildRun = nil
        container = nil
        statusLabel = nil
        // Reclaim the gRPC port for the next boot.
        PreviewShell.freePort(8554)
    }

    /// Pixel-exact annotation frame (identical command on macOS + Linux). The
    /// raw PNG bytes are grabbed off-main (Data is Sendable); the CGImage is
    /// decoded back on the main actor.
    func captureFrame() async -> CGImage? {
        let png = await Self.background {
            PreviewShell.runBinary("adb", ["exec-out", "screencap", "-p"])
        }
        guard let png else { return nil }
        return MacAnnotationRenderer.decode(png)
    }

    private func setStatus(_ text: String) { statusLabel?.stringValue = text }

    // Poll `adb shell getprop sys.boot_completed` == 1 (after `adb wait-for-device`).
    nonisolated private static func waitForBoot(timeout: TimeInterval) -> Bool {
        _ = PreviewShell.run("adb", ["wait-for-device"])
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            let result = PreviewShell.run("adb", ["shell", "getprop", "sys.boot_completed"])
            if result.stdout.trimmingCharacters(in: .whitespacesAndNewlines) == "1" { return true }
            Thread.sleep(forTimeInterval: 2)
        }
        return false
    }

    private static func background<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async { continuation.resume(returning: work()) }
        }
    }
}

// MARK: - iOS Simulator backend (serve-sim MJPEG)

/// Drives `npx serve-sim` (Simulator MJPEG + WS) and renders the MJPEG stream.
/// FIRST CUT: the MJPEG endpoint is rendered in a WKWebView (an <img> pointed at
/// the stream); the clean-frame grab decodes one MJPEG JPEG. The
/// AVSampleBufferDisplayLayer path is the on-Mac iteration.
@MainActor
final class IOSSimPreviewBackend: NSObject, PreviewBackend {
    private var serveSim: PreviewLogRun?
    private var webView: WKWebView?
    private var udid: String?
    private var mjpegURL: String?

    func start(
        context: PreviewRunContext,
        onPhase: @escaping @MainActor (PreviewPhase) -> Void,
        onSurface: @escaping @MainActor (NSView?) -> Void
    ) {
        let target = context.target
        let root = context.resolvedRootDir
        let env = target.env
        let webView = WKWebView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
        webView.setValue(false, forKey: "drawsBackground")
        self.webView = webView
        onSurface(webView)

        Task { @MainActor in
            onPhase(.booting)
            PreviewShell.freePort(3100)
            // serve-sim boots the named simulator + builds/installs the scheme and
            // prints a JSON line { udid, mjpeg, ws }. Run detached so it survives;
            // capture the JSON from its stdout (one-shot run with --detach).
            var args = ["-y", "serve-sim", "--detach", "--quiet"]
            if let scheme = target.scheme { args += ["--scheme", scheme] }
            if let workspace = target.workspace { args += ["--workspace", workspace] }
            if let simulator = target.simulator { args += ["--simulator", simulator] }
            if let bundleId = target.bundleId { args += ["--bundle-id", bundleId] }

            let runArgs = args
            let result = await Self.background { PreviewShell.run("npx", runArgs, cwd: root, env: env) }
            guard result.ok, let info = Self.parseServeSim(result.stdout) else {
                onPhase(.error("serve-sim failed to start.\n\(result.stderr.isEmpty ? result.stdout : result.stderr)"))
                return
            }
            self.udid = info.udid
            self.mjpegURL = info.mjpeg
            // Keep a handle so teardown can --kill even though the worker exited.
            self.serveSim = PreviewLogRun(runId: "preview-ios-\(UUID().uuidString)")

            // Render the MJPEG stream as a full-bleed <img> in the webview (first
            // cut). TODO (on-Mac): decode MJPEG frames into an
            // AVSampleBufferDisplayLayer for a cleaner, lower-overhead embed.
            let html = """
            <!doctype html><html><head><meta name="viewport" content="width=device-width">
            <style>html,body{margin:0;background:#000;height:100%}img{width:100%;height:100%;object-fit:contain;display:block}</style>
            </head><body><img src="\(info.mjpeg)"></body></html>
            """
            webView.loadHTMLString(html, baseURL: URL(string: info.mjpeg))
            onPhase(.running)
        }
    }

    func stop() {
        // serve-sim --kill detaches the stream + the host process; simctl shutdown
        // returns the simulator. Both best-effort, off-main.
        let runningUdid = self.udid
        DispatchQueue.global(qos: .userInitiated).async {
            _ = PreviewShell.run("npx", ["-y", "serve-sim", "--kill"])
            if let runningUdid { _ = PreviewShell.run("xcrun", ["simctl", "shutdown", runningUdid]) }
        }
        serveSim = nil
        webView?.stopLoading()
        webView = nil
        self.udid = nil
        mjpegURL = nil
        PreviewShell.freePort(3100)
    }

    /// One MJPEG JPEG frame for the annotate overlay. The bytes are read off-main
    /// (Sendable Data); the CGImage is decoded back on the main actor.
    func captureFrame() async -> CGImage? {
        guard let mjpeg = mjpegURL, let url = URL(string: mjpeg) else { return nil }
        let jpeg = await Self.background { Self.grabMJPEGFrame(url) }
        guard let jpeg else { return nil }
        return MacAnnotationRenderer.decode(jpeg)
    }

    private struct ServeSimInfo: Sendable { let udid: String; let mjpeg: String; let ws: String? }

    private static func parseServeSim(_ stdout: String) -> ServeSimInfo? {
        // serve-sim prints a JSON object (possibly among other log lines); scan
        // lines for the first that parses with a `mjpeg` field.
        for line in stdout.split(whereSeparator: \.isNewline) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix("{"), let data = trimmed.data(using: .utf8),
                  let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let udid = obj["udid"] as? String, let mjpeg = obj["mjpeg"] as? String else { continue }
            return ServeSimInfo(udid: udid, mjpeg: mjpeg, ws: obj["ws"] as? String)
        }
        return nil
    }

    /// Read the MJPEG multipart stream until the first complete JPEG (SOI..EOI)
    /// and return its bytes. Bounded read so a stalled stream can't block forever.
    nonisolated private static func grabMJPEGFrame(_ url: URL) -> Data? {
        guard let stream = InputStream(url: url) else { return nil }
        stream.open()
        defer { stream.close() }
        var buffer = Data()
        let chunkSize = 16 * 1024
        var scratch = [UInt8](repeating: 0, count: chunkSize)
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline, stream.hasBytesAvailable {
            let read = stream.read(&scratch, maxLength: chunkSize)
            if read <= 0 { break }
            buffer.append(contentsOf: scratch[0..<read])
            if let soi = buffer.firstRange(of: Data([0xFF, 0xD8])),
               let eoi = buffer.firstRange(of: Data([0xFF, 0xD9]), in: soi.lowerBound..<buffer.endIndex) {
                return buffer.subdata(in: soi.lowerBound..<eoi.upperBound)
            }
            if buffer.count > 8 * 1024 * 1024 { break } // safety cap
        }
        return nil
    }

    private static func background<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async { continuation.resume(returning: work()) }
        }
    }
}

// MARK: - Shell convenience

extension PreviewShell {
    /// Run a shell command string via `bash -lc` (login shell so the user's PATH
    /// + nvm/asdf shims resolve). Blocking.
    static func bash(_ command: String, cwd: URL, env: [String: String]) -> Result {
        run("bash", ["-lc", command], cwd: cwd, env: env)
    }
}

private extension Data {
    /// First range of `pattern` within `range`.
    func firstRange(of pattern: Data, in range: Range<Index>) -> Range<Index>? {
        self[range].firstRange(of: pattern)
    }
}
