import XCTest
@testable import ExponentialUIPrimitives

/// #EXP-2204: fixtures/ci-log.json and fixtures/job-graph.json replay
/// through `CiLog` and `JobGraph` (the SDK's src/ci-log.ts + src/job-graph.ts).
final class CiLogTests: XCTestCase {
    private static let fixtures = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("exponential-ui/fixtures")

    private func fixture(_ name: String) throws -> [String: Any] {
        try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: Self.fixtures.appendingPathComponent(name)), options: [.fragmentsAllowed]) as? [String: Any])
    }

    private func num(_ v: Any?) -> Double? { CiLog.number(v) }
    private func int(_ v: Any?) -> Int { Int(num(v) ?? .nan) }

    func testStatusTable() throws {
        let f = try fixture("ci-log.json")
        let t = try XCTUnwrap(f["statusTable"] as? [String: Any])
        XCTAssertEqual(num(t["spinMs"]), CiLog.spinMs)
        for row in try XCTUnwrap(t["rows"] as? [[String: Any]]) {
            let e = CiLog.entry(row["input"] is NSNull ? nil : row["input"])
            XCTAssertEqual(e, CiLog.StatusEntry(status: row["status"] as! String, icon: row["icon"] as! String, spin: row["spin"] as! Bool, color: row["color"] as! String, text: row["text"] as! String, label: row["label"] as! String), "\(row)")
        }
    }

    func testAnchors() throws {
        let a = try XCTUnwrap(try fixture("ci-log.json")["anchors"] as? [String: Any])
        let parse = try XCTUnwrap(a["parse"] as? [[String: Any]])
        XCTAssertGreaterThan(parse.count, 20)
        for c in parse {
            let want = (c["range"] as? [String: Any]).map { CiLog.LineRange(start: int($0["start"]), end: int($0["end"])) }
            XCTAssertEqual(CiLog.parseAnchor(c["text"]), want, "\(c)")
        }
        for c in try XCTUnwrap(a["format"] as? [[String: Any]]) {
            XCTAssertEqual(CiLog.formatAnchor(int(c["start"]), num(c["end"]).map(Int.init)), c["anchor"] as? String)
        }
        for c in try XCTUnwrap(a["extend"] as? [[String: Any]]) {
            XCTAssertEqual(CiLog.extendAnchor(c["current"], line: int(c["line"]), shift: c["shift"] as! Bool), c["anchor"] as? String, "\(c)")
        }
    }

    private func lineJSON(_ l: CiLog.Line) -> NSDictionary {
        ["n": l.n, "section": l.section, "time": l.time ?? NSNull(), "kind": l.kind ?? NSNull(), "text": l.text]
    }

    private func sectionJSON(_ s: CiLog.Section) -> NSDictionary {
        ["index": s.index, "title": s.title, "start": s.start, "end": s.end, "count": s.count, "status": s.status, "duration": s.duration ?? NSNull()]
    }

    private func rowJSON(_ r: CiLog.Row) -> NSDictionary {
        switch r {
        case let .line(n): ["kind": "line", "n": n]
        case let .section(s): ["kind": "section", "section": s]
        }
    }

    func testLogs() throws {
        let logs = try XCTUnwrap(try fixture("ci-log.json")["logs"] as? [[String: Any]])
        XCTAssertGreaterThan(logs.count, 5)
        for c in logs {
            let name = c["name"] as? String ?? "?"
            let model = CiLog.model(c["lines"], sections: c["sections"])
            let want = try XCTUnwrap(c["model"] as? [String: Any])
            XCTAssertEqual(model.lines.map(lineJSON) as NSArray, want["lines"] as? NSArray, name)
            XCTAssertEqual(model.sections.map(sectionJSON) as NSArray, want["sections"] as? NSArray, name)
            XCTAssertEqual(CiLog.openDefault(model), (c["openDefault"] as? [Any] ?? []).map { int($0) }, name)
            for r in try XCTUnwrap(c["rows"] as? [[String: Any]]) {
                XCTAssertEqual(CiLog.rows(model, open: CiLog.openSet(r["open"], model)).map(rowJSON) as NSArray, r["rows"] as? NSArray, "\(name) rows \(r["open"] ?? "")")
            }
            for s in try XCTUnwrap(c["search"] as? [[String: Any]]) {
                let got = CiLog.search(model, query: s["query"]).map { ["line": $0.line, "start": $0.start, "end": $0.end] as NSDictionary }
                XCTAssertEqual(got as NSArray, s["matches"] as? NSArray, "\(name) search \(s["query"] ?? "")")
            }
            for r in try XCTUnwrap(c["reveal"] as? [[String: Any]]) {
                XCTAssertEqual(CiLog.reveal(model, open: r["open"], line: int(r["line"])), (r["result"] as? [Any] ?? []).map { int($0) }, "\(name) reveal")
            }
        }
    }

    func testBlame() throws {
        for c in try XCTUnwrap(try fixture("ci-log.json")["blame"] as? [[String: Any]]) {
            let got = CiLog.blameBlocks(c["blame"], lineCount: int(c["lineCount"])).map { ["block": $0.block, "first": $0.first, "border": $0.border] as NSDictionary }
            XCTAssertEqual(got as NSArray, c["lines"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    private func layoutJSON(_ l: JobGraph.Layout) -> NSDictionary {
        let p = { (pt: CGPoint) -> NSArray in [Double(pt.x), Double(pt.y)] }
        return [
            "mode": l.mode, "width": l.width, "height": l.height,
            "stages": l.stages.map { ["title": $0.title, "x": $0.x] as NSDictionary },
            "nodes": l.nodes.map { n -> NSDictionary in
                ["id": n.id, "name": n.name, "status": n.status, "duration": n.duration ?? NSNull(),
                 "count": n.count.map { ["passed": $0.passed, "total": $0.total] as NSDictionary } ?? NSNull(),
                 "members": n.members, "needs": n.needs, "stage": n.stage, "column": n.column, "row": n.row,
                 "x": n.x, "y": n.y, "width": n.width, "height": n.height]
            },
            "edges": l.edges.map { ["from": $0.from, "to": $0.to, "start": p($0.start), "c1": p($0.c1), "c2": p($0.c2), "end": p($0.end)] as NSDictionary },
        ]
    }

    func testJobGraph() throws {
        let f = try fixture("job-graph.json")
        let g = try XCTUnwrap(f["geometry"] as? [String: Any])
        let geo = JobGraph.geometry
        XCTAssertEqual([geo.nodeWidth, geo.nodeHeight, geo.columnGap, geo.rowGap, geo.stageHeight, geo.stageGap, geo.edgeWidth, geo.rowHeight],
                       ["nodeWidth", "nodeHeight", "columnGap", "rowGap", "stageHeight", "stageGap", "edgeWidth", "rowHeight"].map { num(g[$0])! })
        let cases = try XCTUnwrap(f["cases"] as? [[String: Any]])
        XCTAssertGreaterThan(cases.count, 5)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let graph = JobGraph.layout(c["jobs"], compact: false)
            XCTAssertEqual(layoutJSON(graph), c["graph"] as? NSDictionary, "\(name) graph")
            XCTAssertEqual(layoutJSON(JobGraph.layout(c["jobs"], compact: true)), c["list"] as? NSDictionary, "\(name) list")
            for u in try XCTUnwrap(c["upstream"] as? [[String: Any]]) {
                XCTAssertEqual(JobGraph.upstream(graph, u["id"] as! String), u["ids"] as? [String], "\(name) upstream")
            }
        }
    }
}
