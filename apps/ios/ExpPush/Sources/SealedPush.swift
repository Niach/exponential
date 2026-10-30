import Foundation

/// A sealed push from the relay's accountless mode (EXP-1428/1435):
/// `{"aps":{"alert":{"title":"New message"},"mutable-content":1},"s":"<base64url>"}`.
/// `s` opens (with the device's X25519 secret) to the UTF-8 JSON the web
/// server sealed, `{title, body, data}`, where `data` is the string map a
/// plaintext FCM push carries at the top level of `userInfo` (userId,
/// notificationId, type, issueId, sessionId, …). Opening it yields exactly the
/// content an FCM push would have shown, so taps route identically.
public enum SealedPush {
    /// The `userInfo` key carrying the sealed payload.
    public static let sealedKey = "s"

    public struct Opened {
        public let title: String
        public let body: String?
        public let userInfo: [AnyHashable: Any]
    }

    /// Opens `userInfo["s"]` with `secret`; nil when there is no `s`, it does
    /// not open, or the plaintext is not the expected JSON.
    public static func open(userInfo: [AnyHashable: Any], secret: Data) -> Opened? {
        guard let encoded = userInfo[sealedKey] as? String,
              let sealed = Base64Url.decode(encoded),
              let plaintext = PushCrypto.open(secret: secret, sealed: sealed) else { return nil }
        return map(plaintext: plaintext, userInfo: userInfo)
    }

    /// The opened plaintext → the notification content: `title`/`body` shown,
    /// `data` merged into `userInfo` top-level (like FCM's data keys; the
    /// sealed blob itself is dropped). Separate from `open` so the mapping is
    /// testable without the binding.
    public static func map(plaintext: Data, userInfo: [AnyHashable: Any]) -> Opened? {
        guard let json = try? JSONSerialization.jsonObject(with: plaintext) as? [String: Any],
              let title = json["title"] as? String else { return nil }
        var merged = userInfo
        merged.removeValue(forKey: sealedKey)
        // The /send path copied the display copy into the data keys too
        // (caller keys win), so a sealed push carries the same.
        let body = json["body"] as? String
        merged["title"] = title
        if let body { merged["body"] = body }
        if let data = json["data"] as? [String: Any] {
            for (key, value) in data {
                // The server's data map is strings only; anything else is
                // not part of the contract and is skipped.
                if let string = value as? String { merged[key] = string }
            }
        }
        return Opened(title: title, body: body, userInfo: merged)
    }
}
