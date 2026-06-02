import Foundation

public enum ShapeMessage<T: Sendable>: Sendable {
    case insert(key: String, value: T)
    case update(key: String, value: T)
    case partialUpdate(key: String, columns: Data)
    case delete(key: String, value: T?)
    case upToDate
    case mustRefetch
}
