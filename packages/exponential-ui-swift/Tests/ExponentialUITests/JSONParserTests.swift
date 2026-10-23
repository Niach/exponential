import XCTest
@testable import ExponentialUI

/// VAPP-142: `JSONValue.parse` (the byte parser) answers what the
/// Foundation path answers: every shared fixture, every bench scenario,
/// and the edge cases a props string can carry.
final class JSONParserTests: XCTestCase {
    func testEveryFixtureParsesLikeFoundation() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let dirs = [Fixtures.dir, Fixtures.dir.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("exponential-ui-bench/scenarios")]
        var checked = 0
        for dir in dirs {
            let files = (FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? []).filter { $0.pathExtension == "json" }
            for url in files {
                let text = try String(contentsOf: url, encoding: .utf8)
                XCTAssertTrue(Self.same(JSONValue.parse(text), JSONValue.parseWithFoundation(text)), url.lastPathComponent)
                checked += 1
            }
        }
        XCTAssertGreaterThan(checked, 20)
    }

    func testEdgeCasesParseLikeFoundation() {
        let cases = [
            #"{"a":1,"b":[true,false,null],"c":{"d":"e"}}"#, #"  {"x" : -12.5e3 , "y":0.000001}  "#,
            #""é😀\n\t\"\\\/""#, #"{"emoji":"😀","nbsp":"a b"}"#, "123456789012345678", "9007199254740993",
            "-0", "-1.5E-7", "[]", "{}", "[[[[1]]]]", #"{"a":1,"a":2}"#, "[1,]", #"{"a":1,}"#, "[1e-400]", "true", "null", "\"\"",
            // Malformed: every one is `.null` on both paths.
            "", "{", "[,]", "{,}", "[1,,]", "01", "1.", ".5", "+1", "tru", #""\ud83d""#, #""\x""#, "\"a\nb\"", "{} x", "NaN", "[1 2]", "1E400",
        ]
        for c in cases { XCTAssertTrue(Self.same(JSONValue.parse(c), JSONValue.parseWithFoundation(c)), c) }
        // The one deliberate difference: a 17+ digit integer rounds correctly (Foundation: one ulp low).
        XCTAssertEqual(JSONValue.parse("123456789012345680000"), .number(1.2345678901234568e20))
    }

    /// Equal, a number within one ulp (Foundation's decimal rounding).
    static func same(_ a: JSONValue, _ b: JSONValue) -> Bool {
        switch (a, b) {
        case let (.number(x), .number(y)): return x == y || abs(x - y) <= max(abs(x), abs(y)).ulp
        case let (.array(x), .array(y)): return x.count == y.count && zip(x, y).allSatisfy { same($0, $1) }
        case let (.object(x), .object(y)): return x.count == y.count && x.allSatisfy { k, v in y[k].map { same(v, $0) } ?? false }
        default: return a == b
        }
    }

    func testRoundTripsTheSerializer() {
        let v: JSONValue = .object(["s": .string("ü \"q\" \u{1}"), "n": .number(3.25), "i": .number(-42), "a": .array([.bool(true), .null])])
        XCTAssertEqual(JSONValue.parse(v.json), v)
    }
}

extension JSONParserTests {
    /// The wire writer carries the same VALUE as the Foundation serializer.
    func testWireJSONCarriesTheSameValue() throws {
        let values: [JSONValue] = [
            .object(["s": .string("a/b \"q\" \\ \n\t\u{1}\u{1F600}"), "n": .number(0.1), "big": .number(1e20), "neg": .number(-3), "f": .number(175.862), "e": .number(1e-7)]),
            .array([.null, .bool(false), .object([:]), .array([])]),
        ]
        // A non-finite number is `null` on the wire (Foundation's writer throws on it).
        XCTAssertEqual(JSONValue.array([.number(.infinity), .number(.nan)]).wireJSON, "[null,null]")
        for v in values { XCTAssertEqual(JSONValue.parse(v.wireJSON), JSONValue.parse(v.json), v.wireJSON) }
        XCTAssertEqual(JSONValue.object(["b": .number(1), "a": .number(2)]).wireJSON, #"{"a":2,"b":1}"#)
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        for name in ["kitchen-sink.json", "bench-list.json"] {
            let v = try Fixtures.json(name)
            XCTAssertEqual(JSONValue.parse(v.wireJSON), v, name)
        }
    }
}
