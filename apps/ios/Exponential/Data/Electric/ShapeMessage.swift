import Foundation

enum ShapeMessage<T: Sendable>: Sendable {
    case insert(key: String, value: T)
    case update(key: String, value: T)
    case partialUpdate(key: String, columns: Data)
    case delete(key: String, value: T?)
    case upToDate
    case mustRefetch
}

struct RawShapeMessage: Codable, Sendable {
    let headers: [String: RawJSONValue]?
    let key: String?
    let value: RawJSONValue?
}

// Lightweight JSON value type for parsing Electric shape messages
enum RawJSONValue: Codable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: RawJSONValue])
    case array([RawJSONValue])
    case null

    var stringValue: String? {
        if case let .string(s) = self { return s }
        return nil
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let str = try? container.decode(String.self) {
            self = .string(str)
        } else if let num = try? container.decode(Double.self) {
            self = .number(num)
        } else if let bool = try? container.decode(Bool.self) {
            self = .bool(bool)
        } else if let obj = try? container.decode([String: RawJSONValue].self) {
            self = .object(obj)
        } else if let arr = try? container.decode([RawJSONValue].self) {
            self = .array(arr)
        } else if container.decodeNil() {
            self = .null
        } else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unsupported JSON value")
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .string(s): try container.encode(s)
        case let .number(n): try container.encode(n)
        case let .bool(b): try container.encode(b)
        case let .object(o): try container.encode(o)
        case let .array(a): try container.encode(a)
        case .null: try container.encodeNil()
        }
    }

    var jsonData: Data? {
        switch self {
        case .object, .array:
            return try? JSONEncoder().encode(self)
        default:
            return nil
        }
    }
}
