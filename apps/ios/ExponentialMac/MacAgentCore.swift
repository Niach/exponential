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

    /// Called on the core's event thread. Run requests block, so they run on a
    /// background queue; the parked pipeline thread waits for the result.
    private func dispatch(_ json: String) {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = obj["type"] as? String else { return }
        guard type == "run_request" else {
            onLog?(json)
            return
        }
        let runId = (obj["runId"] as? String) ?? ""
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            let (code, text) = MacAgentRunner.run(obj)
            submitRunResult(runId: runId, exitCode: code, finalText: text)
        }
    }

    /// Stop + free the core and release the callback context. Call exactly once.
    func shutdown() {
        _ = agent_core_stop(core)
        agent_core_free(core)
        ctxToken?.release()
        ctxToken = nil
    }
}

/// Runs one coding-agent CLI invocation and captures its output + exit code.
enum MacAgentRunner {
    static func run(_ obj: [String: Any]) -> (Int32, String) {
        guard let program = obj["program"] as? String else { return (-1, "") }
        let argv = obj["argv"] as? [String] ?? []
        let env = obj["env"] as? [String: String] ?? [:]
        let cwd = obj["cwd"] as? String
        let system = (obj["systemPrompt"] as? String) ?? ""
        let user = (obj["userPrompt"] as? String) ?? ""
        // Same combined-prompt shape the Linux host uses; passed as the final
        // positional argument to the CLI.
        let prompt = "\(system)\n\n<user_issue>\n\(user)\n</user_issue>"

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
