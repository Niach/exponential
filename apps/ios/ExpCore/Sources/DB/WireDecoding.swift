import Foundation

// Electric's shape wire format delivers every column value as a JSON string
// (Postgres text encoding), while tRPC responses and test fixtures use native
// JSON scalars. These helpers accept BOTH forms so each entity decodes its
// non-String columns in a type-aware way — no whole-row blind string coercion,
// which used to convert any numeric/boolean-looking value in ANY field (a title
// "404" / "true" / "3.5") and then drop the row when the String re-decode
// failed. Mirrors the Android field serializers (kotlinx quoted-number parsing
// + PgBoolSerializer).
public extension KeyedDecodingContainer {
    /// Decode an optional Int that may arrive as a JSON number OR a numeric
    /// string. Absent key or JSON null → nil. A present-but-unparseable string
    /// THROWS a dataCorruptedError so the row surfaces as a loud decode drop
    /// (matching Android's throw-and-report) instead of silently becoming nil.
    func decodeWireInt(forKey key: Key) throws -> Int? {
        guard contains(key), try !decodeNil(forKey: key) else { return nil }
        if let i = try? decode(Int.self, forKey: key) { return i }
        let str = try decode(String.self, forKey: key)
        guard let i = Int(str) else {
            throw DecodingError.dataCorruptedError(
                forKey: key, in: self,
                debugDescription: "Expected an integer or numeric string, got \"\(str)\""
            )
        }
        return i
    }

    /// Decode an optional Double that may arrive as a JSON number OR a numeric
    /// string ("3.5", "2", exponent forms). Absent key or JSON null → nil; a
    /// present-but-unparseable string THROWS a dataCorruptedError.
    func decodeWireDouble(forKey key: Key) throws -> Double? {
        guard contains(key), try !decodeNil(forKey: key) else { return nil }
        if let d = try? decode(Double.self, forKey: key) { return d }
        let str = try decode(String.self, forKey: key)
        guard let d = Double(str) else {
            throw DecodingError.dataCorruptedError(
                forKey: key, in: self,
                debugDescription: "Expected a number or numeric string, got \"\(str)\""
            )
        }
        return d
    }

    /// Decode a Bool permissively: a JSON bool → itself; a JSON number → != 0; a
    /// string in {t, true, 1} → true / {f, false, 0} → false (case-insensitive);
    /// anything else — absent, null, or unrecognized — → `def`. Non-throwing to
    /// preserve the default-on-absence semantics pre-rotation rows rely on. The
    /// Swift port of Android's PgBoolSerializer (which also sees bare Postgres
    /// "t"/"f" text off staging).
    func decodeWireBool(forKey key: Key, default def: Bool) -> Bool {
        if let b = try? decode(Bool.self, forKey: key) { return b }
        if let i = try? decode(Int.self, forKey: key) { return i != 0 }
        if let s = try? decode(String.self, forKey: key) {
            switch s.lowercased() {
            case "t", "true", "1": return true
            case "f", "false", "0": return false
            default: return def
            }
        }
        return def
    }

    /// A jsonb column as its stored TEXT form: a pre-stringified JSON string
    /// passes through, an object/array re-encodes type-FAITHFULLY through
    /// JSONWireValue (bools/numbers survive the store-as-string round trip),
    /// and absent/null/undecodable → nil. Non-throwing — a malformed jsonb
    /// value degrades one column, never the row (EXP-481: devices
    /// launch_defaults / worktree agents).
    func decodeWireJsonString(forKey key: Key) -> String? {
        guard contains(key) else { return nil }
        if let stringValue = try? decode(String.self, forKey: key) {
            return stringValue
        }
        if (try? decodeNil(forKey: key)) == true { return nil }
        guard let rawJSON = try? decode(JSONWireValue.self, forKey: key),
              let data = try? JSONEncoder().encode(rawJSON)
        else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

// FEED-33: a Postgres `uuid[]` column (`devices.shared_team_ids`) reaches the
// client in three shapes — the Electric wire's text literal (`{a,b}`, `{}`,
// elements possibly double-quoted), a JSON string array (tRPC fixtures, and
// GRDB's own JSON encoding of the stored `[String]`), or nothing at all on a
// pre-FEED-33 server. All of them read as a plain id list; anything else is
// an empty one, never a dropped row.
public enum PgTextArray {
    public static func parse(_ raw: String) -> [String] {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("[") {
            let list = (try? JSONDecoder().decode([String].self, from: Data(trimmed.utf8))) ?? []
            return clean(list)
        }
        guard trimmed.hasPrefix("{"), trimmed.hasSuffix("}") else { return [] }
        let inner = trimmed.dropFirst().dropLast()
        return clean(inner.split(separator: ",").map(String.init))
    }

    private static func clean(_ items: [String]) -> [String] {
        let strip = CharacterSet.whitespacesAndNewlines.union(CharacterSet(charactersIn: "\""))
        return items
            .map { $0.trimmingCharacters(in: strip) }
            .filter { !$0.isEmpty }
    }
}

public extension KeyedDecodingContainer {
    /// A Postgres text-array / JSON string-array column as `[String]` (see
    /// `PgTextArray`). Non-throwing: absent, null or garbage → `[]`.
    func decodeWireStringList(forKey key: Key) -> [String] {
        guard contains(key) else { return [] }
        if let text = try? decode(String.self, forKey: key) {
            return PgTextArray.parse(text)
        }
        if let list = try? decode([String].self, forKey: key) {
            return list.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        }
        return []
    }
}
