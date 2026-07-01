import Foundation

/// Tails a growing file on a background queue and delivers each appended chunk.
/// Used to drain the `script(1)` typescript the coding launcher writes when
/// steering is enabled — the only clean way to tee a libghostty-owned PTY on
/// macOS (ghostty exposes no output-read API; see MacSteerPublisher). The file
/// carries verbatim terminal bytes, so a remote xterm.js renders identically.
final class MacSteerPtyTail: @unchecked Sendable {
    private let path: String
    private let onChunk: @Sendable (Data) -> Void
    private let queue = DispatchQueue(label: "at.exponential.steer.ptytail", qos: .userInitiated)
    private var stopped = false
    private var handle: FileHandle?

    init(path: String, onChunk: @escaping @Sendable (Data) -> Void) {
        self.path = path
        self.onChunk = onChunk
    }

    func start() {
        queue.async { [weak self] in self?.loop() }
    }

    func stop() {
        queue.async { [weak self] in
            self?.stopped = true
            try? self?.handle?.close()
            self?.handle = nil
        }
    }

    private func loop() {
        // The launcher creates the file when it spawns; wait briefly for it.
        var waited = 0
        while !stopped, !FileManager.default.fileExists(atPath: path), waited < 200 {
            Thread.sleep(forTimeInterval: 0.05)
            waited += 1
        }
        guard !stopped, let h = FileHandle(forReadingAtPath: path) else { return }
        handle = h
        // Poll-tail: FileHandle tracks the read offset, so once we hit EOF a later
        // read resumes from where the writer appended. Sleep on EOF to avoid a busy
        // loop without adding meaningful latency to live output.
        while !stopped {
            let chunk = h.availableData
            if chunk.isEmpty {
                Thread.sleep(forTimeInterval: 0.03)
                continue
            }
            if !stopped { onChunk(chunk) }
        }
    }
}
