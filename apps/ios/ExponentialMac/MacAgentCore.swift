import AgentCore
import Foundation

/// Thin Swift wrapper over the Rust `agent-core` C ABI. The core owns its own
/// background runtime; outbound events arrive on the C callback (a NON-main
/// thread). `run_request` events are fulfilled here by running the coding CLI
/// and reporting completion via `agent_core_submit_run_result` (which unblocks
/// the core's parked pipeline thread). Mirrors `apps/linux/.../agent_manager.zig`,
/// but uses `Foundation.Process` (real exit code + captured output) instead of
/// the embedded terminal — the visible "watch & steer" terminal is M7/deferred.
final class MacAgentCore: @unchecked Sendable {
    private let core: OpaquePointer
    private let onLog: (@Sendable (String) -> Void)?
    private var ctxToken: Unmanaged<MacAgentCore>?
    private var hasShutdown = false

    init?(configJson: String, onLog: (@Sendable (String) -> Void)? = nil) {
        guard let core = configJson.withCString({ agent_core_create($0) }) else { return nil }
        self.core = core
        self.onLog = onLog
        self.ctxToken = nil

        let token = Unmanaged.passRetained(self)
        self.ctxToken = token
        agent_core_set_event_callback(core, token.toOpaque()) { ctx, json, len in
            guard let ctx, let json else { return }
            let me = Unmanaged<MacAgentCore>.fromOpaque(ctx).takeUnretainedValue()
            let text = String(decoding: UnsafeRawBufferPointer(start: json, count: len), as: UTF8.self)
            me.dispatch(text)
        }
        _ = agent_core_start(core)
    }

    private func submitRunResult(runId: String, exitCode: Int32, finalText: String) {
        runId.withCString { rid in
            finalText.withCString { ft in
                _ = agent_core_submit_run_result(core, rid, exitCode, ft)
            }
        }
    }

    /// Called on the core's event thread. A run_request is fulfilled in a visible
    /// ghostty terminal on the main thread; the parked pipeline thread waits for
    /// `submit_run_result`.
    private func dispatch(_ json: String) {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = obj["type"] as? String else { return }
        guard type == "run_request" else {
            onLog?(json)
            return
        }
        let runId = (obj["runId"] as? String) ?? ""
        guard let program = obj["program"] as? String, !program.isEmpty else {
            submitRunResult(runId: runId, exitCode: -1, finalText: "")
            return
        }
        let argv = (obj["argv"] as? [String]) ?? []
        let env = (obj["env"] as? [String: String]) ?? [:]
        let cwd = obj["cwd"] as? String
        let system = (obj["systemPrompt"] as? String) ?? ""
        let user = (obj["userPrompt"] as? String) ?? ""
        let prompt = "\(system)\n\n<user_issue>\n\(user)\n</user_issue>"
        DispatchQueue.main.async { [self] in
            MacAgentTerminalRunner.shared.run(
                runId: runId, program: program, argv: argv, env: env, cwd: cwd, prompt: prompt
            ) { [self] code, text in
                submitRunResult(runId: runId, exitCode: code, finalText: text)
            }
        }
    }

    /// Stop + free the core and release the callback context. Idempotent.
    func shutdown() {
        guard !hasShutdown else { return }
        hasShutdown = true
        _ = agent_core_stop(core)
        agent_core_free(core)
        ctxToken?.release()
        ctxToken = nil
    }

    deinit {
        // Safety net if shutdown() was never called (e.g. abandoned without
        // unregister) — avoids leaking the core + the retained callback context.
        if !hasShutdown {
            _ = agent_core_stop(core)
            agent_core_free(core)
            ctxToken?.release()
        }
    }
}

/// Headless fallback: runs one coding-agent CLI invocation and captures its
/// output + exit code (used when the ghostty terminal is unavailable).
enum MacAgentRunner {
    static func run(program: String, argv: [String], env: [String: String], cwd: String?, prompt: String) -> (Int32, String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [program] + argv + [prompt]
        if let cwd { process.currentDirectoryURL = URL(fileURLWithPath: cwd) }

        var environment = ProcessInfo.processInfo.environment
        // GUI apps launch with a minimal PATH; add the usual CLI install dirs so
        // `claude`/`codex` resolve.
        let basePath = environment["PATH"].map { $0 + ":" } ?? ""
        environment["PATH"] = basePath + "/opt/homebrew/bin:/usr/local/bin:\(NSHomeDirectory())/.local/bin"
        for (k, v) in env { environment[k] = v }
        process.environment = environment

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        do {
            try process.run()
        } catch {
            return (-1, "Failed to launch \(program): \(error.localizedDescription)")
        }
        // Read to EOF (the write end closes when the child exits) before waiting,
        // so large output never deadlocks on a full pipe buffer.
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus, String(data: data, encoding: .utf8) ?? "")
    }
}
