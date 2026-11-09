import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// #EXP-1629: a KeyRecorder `field` while it records: a platform view over
/// the field takes the first responder (the field takes the focus) and
/// swallows EVERY key (Tab, Escape and shortcuts too; repeats send
/// nothing), handing the canonical payload to `SurfaceModel
/// .keyRecorderRecord`; losing the first responder fires `blur`. Idle, the
/// field is a plain pressable Box (its tap is the generic press). The look
/// (the ring border while recording) is the recipe's, painted like any Box.
struct KeyRecorderCaptureModifier: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel
    let enabled: Bool

    func body(content: Content) -> some View {
        if enabled, SurfaceModel.isKeyRecorderField(node), model.keyRecorderRecording(node.index) {
            let id = node.id
            content.overlay {
                KeyRecorderCaptureView(
                    onInput: { model.keyRecorderRecord(id: id, $0) },
                    onBlur: { model.keyRecorderBlur(id: id) }
                )
                .accessibilityHidden(true)
            }
        } else {
            content
        }
    }
}

#if canImport(UIKit)
/// iOS / iPadOS: a hardware keyboard's presses (`UIKey.keyCode`, the HID
/// usage = the physical key) and the pointer's buttons 3-5
/// (`UIEvent.buttonMask`, an iPad mouse where UIKit delivers them). A
/// `UIKeyCommand` the app registers still matches before `pressesBegan`.
struct KeyRecorderCaptureView: UIViewRepresentable {
    let onInput: (KeyRecorderInput) -> Void
    let onBlur: () -> Void

    func makeUIView(context: Context) -> CaptureView {
        let v = CaptureView()
        v.backgroundColor = .clear
        v.isAccessibilityElement = false
        v.onInput = onInput
        v.onBlur = onBlur
        return v
    }

    func updateUIView(_ v: CaptureView, context: Context) {
        v.onInput = onInput
        v.onBlur = onBlur
    }

    static func dismantleUIView(_ v: CaptureView, coordinator: ()) {
        // Gone because recording ended: no `blur` (the core ignores it anyway).
        v.onInput = nil
        v.onBlur = nil
        if v.isFirstResponder { _ = v.resignFirstResponder() }
    }

    final class CaptureView: UIView {
        var onInput: ((KeyRecorderInput) -> Void)?
        var onBlur: (() -> Void)?
        /// The modifier HID usages held now (left and right apart).
        private var held: Set<Int> = []

        override var canBecomeFirstResponder: Bool { true }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window != nil { DispatchQueue.main.async { [weak self] in _ = self?.becomeFirstResponder() } }
        }

        override func resignFirstResponder() -> Bool {
            let resigned = super.resignFirstResponder()
            if resigned { onBlur?() }
            return resigned
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            for p in presses { key(p, down: true) }
        }

        override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            for p in presses { key(p, down: false) }
        }

        override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            for p in presses {
                if let k = p.key { held.remove(k.keyCode.rawValue) }
            }
        }

        // Swallowed: nothing else sees a key while recording.
        override func pressesChanged(_ presses: Set<UIPress>, with event: UIPressesEvent?) {}

        private func key(_ p: UIPress, down: Bool) {
            guard let k = p.key else { return }
            let usage = k.keyCode.rawValue
            var f = Self.flags(k.modifierFlags)
            if let m = KeyRecorderKeys.modifier(hidUsage: usage) {
                if down { held.insert(usage) } else { held.remove(usage) }
                // The flags AFTER the change: this side's modifier follows the
                // press, still on while its twin is held.
                let on = held.contains { KeyRecorderKeys.modifier(hidUsage: $0) == m }
                switch m {
                case .ctrl: f.ctrl = on
                case .shift: f.shift = on
                case .alt: f.alt = on
                case .meta: f.meta = on
                }
                onInput?(KeyRecorderInput(type: .modifiers, flags: f))
                return
            }
            guard let name = KeyRecorderKeys.name(hidUsage: usage) else { return }
            onInput?(KeyRecorderInput(type: down ? .down : .up, key: name, flags: f))
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
            // A tap while recording does nothing; buttons 3-5 go to the host.
            guard let mask = event?.buttonMask else { return }
            for b in 3...5 where mask.contains(.button(b)) {
                onInput?(KeyRecorderInput(type: .mouse, button: b, flags: Self.flags(event?.modifierFlags ?? [])))
            }
        }

        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {}
        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {}
        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {}

        static func flags(_ m: UIKeyModifierFlags) -> KeyRecorderFlags {
            KeyRecorderFlags(ctrl: m.contains(.control), alt: m.contains(.alternate), shift: m.contains(.shift), meta: m.contains(.command))
        }
    }
}
#elseif canImport(AppKit)
/// macOS: `keyDown` / `keyUp` (`NSEvent.keyCode`, the ANSI position = the
/// physical key), `flagsChanged` for a modifier alone, key equivalents
/// (Cmd shortcuts) swallowed, `otherMouseDown` for buttons 3-5. Losing the
/// first responder, the window losing key, or a click outside the field =
/// `blur`.
struct KeyRecorderCaptureView: NSViewRepresentable {
    let onInput: (KeyRecorderInput) -> Void
    let onBlur: () -> Void

    func makeNSView(context: Context) -> CaptureView {
        let v = CaptureView()
        v.onInput = onInput
        v.onBlur = onBlur
        return v
    }

    func updateNSView(_ v: CaptureView, context: Context) {
        v.onInput = onInput
        v.onBlur = onBlur
    }

    static func dismantleNSView(_ v: CaptureView, coordinator: ()) {
        // Gone because recording ended: no `blur` (the core ignores it anyway).
        v.onInput = nil
        v.onBlur = nil
        v.stop()
    }

    final class CaptureView: NSView {
        var onInput: ((KeyRecorderInput) -> Void)?
        var onBlur: (() -> Void)?
        private var monitor: Any?
        private var resignObserver: NSObjectProtocol?

        override var acceptsFirstResponder: Bool { true }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window else {
                stop()
                return
            }
            window.makeFirstResponder(self)
            resignObserver = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.blurOnce() }
            }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) { [weak self] e in
                MainActor.assumeIsolated {
                    guard let self, e.window === self.window else { return }
                    let inside = self.bounds.contains(self.convert(e.locationInWindow, from: nil))
                    if !inside { self.blurOnce() }
                }
                return e
            }
        }

        func stop() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            if let resignObserver { NotificationCenter.default.removeObserver(resignObserver) }
            resignObserver = nil
        }

        private func blurOnce() {
            let blur = onBlur
            onBlur = nil
            stop()
            blur?()
        }

        override func resignFirstResponder() -> Bool {
            let resigned = super.resignFirstResponder()
            if resigned { blurOnce() }
            return resigned
        }

        override func keyDown(with e: NSEvent) { key(e, down: true) }
        override func keyUp(with e: NSEvent) { key(e, down: false) }

        override func performKeyEquivalent(with e: NSEvent) -> Bool {
            // A shortcut is a key while recording (no keyDown follows it).
            guard window?.firstResponder === self else { return false }
            key(e, down: true)
            return true
        }

        override func flagsChanged(with e: NSEvent) {
            guard KeyRecorderKeys.modifier(macKeyCode: Int(e.keyCode)) != nil else { return }
            onInput?(KeyRecorderInput(type: .modifiers, flags: Self.flags(e.modifierFlags)))
        }

        private func key(_ e: NSEvent, down: Bool) {
            if e.isARepeat { return }
            guard let name = KeyRecorderKeys.name(macKeyCode: Int(e.keyCode)) else { return }
            onInput?(KeyRecorderInput(type: down ? .down : .up, key: name, flags: Self.flags(e.modifierFlags)))
        }

        // A click on the field while recording does nothing.
        override func mouseDown(with e: NSEvent) {}
        override func rightMouseDown(with e: NSEvent) {}

        override func otherMouseDown(with e: NSEvent) {
            guard let b = KeyRecorderKeys.button(macButtonNumber: e.buttonNumber) else { return }
            onInput?(KeyRecorderInput(type: .mouse, button: b, flags: Self.flags(e.modifierFlags)))
        }

        static func flags(_ m: NSEvent.ModifierFlags) -> KeyRecorderFlags {
            KeyRecorderFlags(ctrl: m.contains(.control), alt: m.contains(.option), shift: m.contains(.shift), meta: m.contains(.command))
        }
    }
}
#endif
