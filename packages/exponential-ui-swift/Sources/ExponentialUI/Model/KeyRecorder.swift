import SwiftUI
import ExponentialUICore

/// #EXP-1629 (`fixtures/interactions-round1.json` `keyRecorder`): a
/// KeyRecorder's `field` part captures; the HOST owns the grammar. The
/// painter sends `press` (a tap, Enter / Space while idle: the generic
/// press), `record` with the canonical payload while it records, and
/// `blur` when it loses the focus; the core keeps the guards (a press only
/// while idle, `record` / `blur` only while recording) and fires `start` /
/// `key` / `stop` with the payload as the context.
public struct KeyRecorderInput: Equatable, Sendable {
    public enum Kind: String, Sendable { case down, up, modifiers, mouse }
    public var type: Kind
    /// The canonical key name (`down` / `up`).
    public var key: String?
    /// The mouse button, 3 middle, 4 back, 5 forward (`mouse`).
    public var button: Int?
    public var flags: KeyRecorderFlags

    public init(type: Kind, key: String? = nil, button: Int? = nil, flags: KeyRecorderFlags = KeyRecorderFlags()) {
        self.type = type
        self.key = key
        self.button = button
        self.flags = flags
    }

    /// The `record` payload: `{type, key?, button?, ctrl, alt, shift, meta}`.
    public var payload: JSONValue {
        var o: [String: JSONValue] = [
            "type": .string(type.rawValue),
            "ctrl": .bool(flags.ctrl),
            "alt": .bool(flags.alt),
            "shift": .bool(flags.shift),
            "meta": .bool(flags.meta),
        ]
        if let key { o["key"] = .string(key) }
        if let button { o["button"] = .number(Double(button)) }
        return .object(o)
    }
}

/// The held modifiers (meta = Cmd / Super / Win).
public struct KeyRecorderFlags: Equatable, Sendable {
    public var ctrl = false
    public var alt = false
    public var shift = false
    public var meta = false

    public init(ctrl: Bool = false, alt: Bool = false, shift: Bool = false, meta: Bool = false) {
        self.ctrl = ctrl
        self.alt = alt
        self.shift = shift
        self.meta = meta
    }
}

/// The CANONICAL key names, the same on every platform: a-z, 0-9 (the
/// physical key, never the shifted character), f1-f24, the named keys and
/// the US-layout punctuation of the key; anything else = nil (not sent).
/// Pure tables on the raw codes, so they test on any platform.
public enum KeyRecorderKeys {
    /// A modifier of a HID usage (keyboard page 0x07: 0xE0…0xE7).
    public enum Modifier: Sendable { case ctrl, shift, alt, meta }

    /// `UIKey.keyCode.rawValue` (UIKeyboardHIDUsage, HID page 0x07) → name.
    public static func name(hidUsage u: Int) -> String? {
        switch u {
        case 0x04...0x1D: return String(Character(Unicode.Scalar(UInt8(0x61 + u - 0x04))))
        case 0x1E...0x26: return String(u - 0x1E + 1)
        case 0x27: return "0"
        case 0x28, 0x58: return "enter"
        case 0x29: return "escape"
        case 0x2A: return "backspace"
        case 0x2B: return "tab"
        case 0x2C: return "space"
        case 0x2D: return "-"
        case 0x2E: return "="
        case 0x2F: return "["
        case 0x30: return "]"
        case 0x31: return "\\"
        case 0x33: return ";"
        case 0x34: return "'"
        case 0x35: return "`"
        case 0x36: return ","
        case 0x37: return "."
        case 0x38: return "/"
        case 0x3A...0x45: return "f\(u - 0x3A + 1)"
        case 0x49: return "insert"
        case 0x4A: return "home"
        case 0x4B: return "pageup"
        case 0x4C: return "delete"
        case 0x4D: return "end"
        case 0x4E: return "pagedown"
        case 0x4F: return "right"
        case 0x50: return "left"
        case 0x51: return "down"
        case 0x52: return "up"
        case 0x68...0x73: return "f\(u - 0x68 + 13)"
        default: return nil
        }
    }

    /// The modifier a HID usage is (left / right Control, Shift, Alt, GUI).
    public static func modifier(hidUsage u: Int) -> Modifier? {
        switch u {
        case 0xE0, 0xE4: .ctrl
        case 0xE1, 0xE5: .shift
        case 0xE2, 0xE6: .alt
        case 0xE3, 0xE7: .meta
        default: nil
        }
    }

    /// `NSEvent.keyCode` (the ANSI positions, `kVK_*`) → name. The Mac has
    /// no F21-F24; Help sits where a PC keyboard has Insert.
    public static func name(macKeyCode c: Int) -> String? {
        macNames[c]
    }

    /// The modifier a Mac key code is (left / right Command, Shift, Option, Control).
    public static func modifier(macKeyCode c: Int) -> Modifier? {
        switch c {
        case 0x37, 0x36: .meta
        case 0x38, 0x3C: .shift
        case 0x3A, 0x3D: .alt
        case 0x3B, 0x3E: .ctrl
        default: nil
        }
    }

    /// `NSEvent.buttonNumber` (0 left, 1 right, 2 middle, 3 back, 4
    /// forward) → the canonical button; nil = sends nothing (1 and 2).
    public static func button(macButtonNumber b: Int) -> Int? {
        (2...4).contains(b) ? b + 1 : nil
    }

    private static let macNames: [Int: String] = [
        0x00: "a", 0x0B: "b", 0x08: "c", 0x02: "d", 0x0E: "e", 0x03: "f", 0x05: "g", 0x04: "h",
        0x22: "i", 0x26: "j", 0x28: "k", 0x25: "l", 0x2E: "m", 0x2D: "n", 0x1F: "o", 0x23: "p",
        0x0C: "q", 0x0F: "r", 0x01: "s", 0x11: "t", 0x20: "u", 0x09: "v", 0x0D: "w", 0x07: "x",
        0x10: "y", 0x06: "z",
        0x1D: "0", 0x12: "1", 0x13: "2", 0x14: "3", 0x15: "4", 0x17: "5", 0x16: "6", 0x1A: "7",
        0x1C: "8", 0x19: "9",
        0x7A: "f1", 0x78: "f2", 0x63: "f3", 0x76: "f4", 0x60: "f5", 0x61: "f6", 0x62: "f7",
        0x64: "f8", 0x65: "f9", 0x6D: "f10", 0x67: "f11", 0x6F: "f12", 0x69: "f13", 0x6B: "f14",
        0x71: "f15", 0x6A: "f16", 0x40: "f17", 0x4F: "f18", 0x50: "f19", 0x5A: "f20",
        0x31: "space", 0x24: "enter", 0x4C: "enter", 0x30: "tab", 0x35: "escape", 0x33: "backspace",
        0x75: "delete", 0x72: "insert", 0x73: "home", 0x77: "end", 0x74: "pageup", 0x79: "pagedown",
        0x7E: "up", 0x7D: "down", 0x7B: "left", 0x7C: "right",
        0x1B: "-", 0x18: "=", 0x21: "[", 0x1E: "]", 0x2A: "\\", 0x29: ";", 0x27: "'",
        0x2B: ",", 0x2F: ".", 0x2C: "/", 0x32: "`",
    ]
}

extension SurfaceModel {
    /// Is node `index` a KeyRecorder's `field`?
    public func isKeyRecorderField(_ index: Int) -> Bool {
        guard let n = node(index) else { return false }
        return Self.isKeyRecorderField(n)
    }

    static func isKeyRecorderField(_ n: NodeInfo) -> Bool {
        n.part == "field" && (n.ownerComponent == "KeyRecorder" || n.macroName == "KeyRecorder")
    }

    /// The field records (its `recording` recipe prop, read live).
    public func keyRecorderRecording(_ index: Int) -> Bool {
        isKeyRecorderField(index) && ownerProps(index)["recording"]?.bool == true
    }

    /// A press of the field (the generic press: the core starts only while idle).
    public func keyRecorderPress(id: String) {
        press(id: id)
    }

    /// A key, a modifier change or a mouse button while recording. Buttons
    /// 1 and 2 send nothing; the core drops it unless the field records.
    public func keyRecorderRecord(id: String, _ input: KeyRecorderInput) {
        guard let i = byId[id], isKeyRecorderField(i) else { return }
        if input.type == .mouse, !(3...5).contains(input.button ?? 0) { return }
        if input.type == .down || input.type == .up, input.key == nil { return }
        fire(i, "record", payload: input.payload)
    }

    /// The field lost the focus (the core stops only while it records).
    public func keyRecorderBlur(id: String) {
        guard let i = byId[id], isKeyRecorderField(i) else { return }
        fire(i, "blur")
    }

    /// A press anywhere else in the surface takes the focus from a
    /// recording field (the web's blur on a click elsewhere).
    func keyRecorderBlurOthers(pressing index: Int) {
        for n in nodes where n.index != index && !n.removed && keyRecorderRecording(n.index) {
            fire(n.index, "blur")
        }
    }
}
