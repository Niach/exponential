import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// #VAPP-258 (`catalog/host.json` markdown, `fixtures/markdown-host.json`):
/// the host hooks a Markdown asks (chips, imageSize, onImagePress,
/// attachment) through the painter's pipeline, and the measurer agreeing.
@MainActor
final class MarkdownHostTests: XCTestCase {
    /// The fixture's FAKE HOST.
    final class FakeHost: HostPlugin {
        let links: [(key: String, href: String, chip: TextLinkChip?)]
        let images: [String: CGSize]
        let attachments: [String: MarkdownAttachmentInfo]
        let urlPolicy: UrlPolicy?
        var pressed: [String] = []
        var pressable = false

        init(_ f: JSONValue) {
            let raw = f["links"]?.object ?? [:]
            links = raw.keys.sorted().map { key in
                let v = raw[key]!
                let chip = v["chip"].flatMap { c -> TextLinkChip? in
                    guard c.object != nil else { return nil }
                    return TextLinkChip(kind: c["kind"]?.string ?? "", label: c["label"]?.string ?? "", icon: c["icon"]?.string, ident: c["ident"]?.string)
                }
                return (key, v["href"]?.string ?? "", chip)
            }
            images = (f["images"]?.object ?? [:]).mapValues { CGSize(width: $0["width"]?.number ?? 0, height: $0["height"]?.number ?? 0) }
            attachments = (f["attachments"]?.object ?? [:]).mapValues { MarkdownAttachmentInfo(kind: $0["kind"]?.string ?? "", name: $0["name"]?.string ?? "") }
            urlPolicy = UrlPolicy(baseUrl: f["policy"]?["baseUrl"]?.string, linkBase: f["policy"]?["linkBase"]?.string)
        }

        func textLinks(_ text: String) -> [TextLinkSegment]? {
            var out: [TextLinkSegment] = []
            var plain = ""
            var rest = Substring(text)
            var found = false
            while !rest.isEmpty {
                if let hit = links.first(where: { rest.hasPrefix($0.key) }) {
                    if !plain.isEmpty { out.append(TextLinkSegment(text: plain)); plain = "" }
                    out.append(TextLinkSegment(text: hit.key, href: hit.href, chip: hit.chip))
                    rest = rest.dropFirst(hit.key.count)
                    found = true
                } else {
                    plain.append(rest.removeFirst())
                }
            }
            if !plain.isEmpty { out.append(TextLinkSegment(text: plain)) }
            return found ? out : nil
        }

        func imageSize(_ src: String) -> CGSize? { images[src] }
        func attachment(_ href: String) -> MarkdownAttachmentInfo? { attachments[href] }
        var onImagePress: (@MainActor (String) -> Void)? {
            guard pressable else { return nil }
            return { [unowned self] (src: String) -> Void in self.pressed.append(src) }
        }
    }

    static func runs(_ inlines: [MarkdownInline]) -> [[String: Any]] {
        var out: [[String: Any]] = []
        var lastKey: String?
        for run in inlines where !run.text.isEmpty {
            let key = "\(run.bold)\(run.italic)\(run.code)\(run.strike)\(run.link ?? "\u{0}")"
            if run.chip == nil, key == lastKey, var last = out.popLast() {
                last["text"] = (last["text"] as! String) + run.text
                out.append(last)
                continue
            }
            var r: [String: Any] = ["text": run.text]
            if run.bold { r["bold"] = true }
            if run.italic { r["italic"] = true }
            if run.code { r["code"] = true }
            if run.strike { r["strike"] = true }
            if let link = run.link { r["link"] = link }
            if let chip = run.chip {
                var c: [String: Any] = ["kind": chip.kind, "label": chip.label]
                if let icon = chip.icon { c["icon"] = icon }
                if let ident = chip.ident { c["ident"] = ident }
                r["chip"] = c
            }
            out.append(r)
            lastKey = run.chip == nil ? key : nil
        }
        return out
    }

    static func normal(_ blocks: [MarkdownBlock]) -> [[String: Any]] {
        blocks.map { b -> [String: Any] in
            switch b.kind {
            case .paragraph: return ["kind": "paragraph", "runs": runs(b.inlines)]
            case .quote: return ["kind": "quote", "runs": runs(b.inlines)]
            case let .heading(level): return ["kind": "heading", "level": level, "runs": runs(b.inlines)]
            case let .listItem(marker, task):
                var o: [String: Any] = ["kind": "listItem", "marker": marker, "depth": b.depth, "runs": runs(b.inlines)]
                if let task { o["task"] = task }
                return o
            case .codeBlock:
                var o: [String: Any] = ["kind": "codeBlock", "text": b.inlines.map(\.text).joined()]
                if let lang = b.lang { o["lang"] = lang }
                return o
            case let .table(header, rows):
                return ["kind": "table", "header": header.map(runs), "rows": rows.map { $0.map(runs) }]
            case .rule: return ["kind": "rule"]
            case let .image(src, alt):
                var o: [String: Any] = ["kind": "image", "src": src, "alt": alt]
                if let size = b.imageSize {
                    o["width"] = Double(size.width)
                    o["height"] = Double(size.height)
                }
                return o
            case let .attachment(type, name, href): return ["kind": "attachment", "type": type.rawValue, "name": name, "href": href]
            }
        }
    }

    func fixture() throws -> (JSONValue, [String: Any]) {
        let raw = try JSONSerialization.jsonObject(with: try Data(contentsOf: Fixtures.dir.appendingPathComponent("markdown-host.json"))) as! [String: Any]
        return (try Fixtures.json("markdown-host.json"), raw)
    }

    func model(_ host: HostPlugin, theme: Bool = true) throws -> SurfaceModel {
        var o = SurfaceOptions()
        if theme { o.theme = ThemeHandle.builtin("exponential") }
        return try SurfaceModel(id: "md", options: o, host: host)
    }

    /// `cases`: the painter's blocks after the fake host's hooks.
    func testEveryCaseMatchesTheNormalForm() throws {
        guard Fixtures.available() else { return }
        let (f, raw) = try fixture()
        let m = try model(FakeHost(f))
        let cases = try XCTUnwrap(raw["cases"] as? [[String: Any]])
        XCTAssertGreaterThan(cases.count, 20)
        for c in cases {
            let name = c["name"] as! String
            let actual = Self.normal(MarkdownPainter.painted(c["text"] as! String, model: m)) as NSArray
            XCTAssertEqual(actual, c["blocks"] as! NSArray, name)
        }
    }

    /// `imageBoxes`: the box the layout gives a block image.
    func testImageBoxes() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        for b in f["imageBoxes"]?.array ?? [] {
            func pair(_ v: JSONValue?) -> CGSize? { v.flatMap { s -> CGSize? in s.object == nil ? nil : CGSize(width: s["width"]?.number ?? 0, height: s["height"]?.number ?? 0) } }
            // `loaded`: an unknown (or invalid) host size takes the decoded one.
            let size = Markdown.imageSize(host: pair(b["size"]), loaded: pair(b["loaded"]))
            let available = CGFloat(b["available"]?.number ?? 0)
            var block = MarkdownBlock(kind: .image(src: "x", alt: ""), inlines: [])
            block.imageSize = size.flatMap { $0.width > 0 && $0.height > 0 ? $0 : nil }
            var styles = MarkdownStyles(body: MarkdownTextSpec(size: 14, lineHeight: 20, weight: 400, family: nil))
            // `maxHeight`: absent = 160, null = none; `viewport`: absent = unknown.
            if let cap = b.object?["maxHeight"] { styles.imageHeight = cap.number.map { CGFloat($0) } }
            styles.viewportHeight = b["viewport"]?.number.map { CGFloat($0) }
            let laid = Markdown.layout([block], styles, width: available, text: MarkdownShaper(mono: nil))
            let name = b["name"]?.string ?? "?"
            XCTAssertEqual(laid.blocks[0].width ?? available, CGFloat(b["box"]?["width"]?.number ?? -1), accuracy: 0.01, name)
            XCTAssertEqual(laid.blocks[0].height, CGFloat(b["box"]?["height"]?.number ?? -1), accuracy: 0.01, name)
        }
    }

    /// No hook, no answer: exactly today's blocks.
    func testNoHostIsToday() throws {
        let m = try model(NoHost())
        let text = "see #ABC-1 now\n\n![x](https://cdn.example/p.png)\n\n[clip](https://cdn.example/clip.mp4)"
        let blocks = MarkdownPainter.painted(text, model: m)
        XCTAssertEqual(blocks, Markdown.mapLinks(MarkdownPainter.blocks(text, model: m), m.linkHref))
        XCTAssertNil(m.host.onImagePress)
    }

    /// The measured height of a Markdown root (the surface's frame).
    func measured(_ m: SurfaceModel, _ text: String, width: CGFloat = 320) throws -> CGFloat {
        try m.setComponents(json: JSONValue.array([.object(["id": .string("root"), "component": .string("Markdown"), "text": .string(text)])]).json)
        m.setViewport(width: width, height: 900)
        let root = try XCTUnwrap(m.nodes.first { $0.id == "root" })
        return m.frame(root.index).height
    }

    /// The painter's layout of the same text at the root's width.
    func laidOut(_ m: SurfaceModel, _ text: String) throws -> (MarkdownLayout, MarkdownStyles, [MarkdownBlock], CGFloat) {
        let root = try XCTUnwrap(m.nodes.first { $0.id == "root" })
        let styles = MarkdownPainter.styles(theme: m.theme, mode: m.mode, body: m.textStyle(root.index), props: root.props)
        let blocks = MarkdownPainter.painted(text, model: m)
        let width = m.frame(root.index).width
        return (Markdown.layout(blocks, styles, width: width, text: MarkdownShaper(mono: m.theme?.monoFamily, chip: styles.chip)), styles, blocks, width)
    }

    func testChipMeasureEqualsPaint() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        let m = try model(FakeHost(f))
        // A chip that fits: one line, as tall as the plain paragraph.
        let chipped = try measured(m, "see #ABC-1 now")
        let (laid, styles, blocks, width) = try laidOut(m, "see #ABC-1 now")
        XCTAssertEqual(chipped, laid.height, accuracy: 0.5)
        XCTAssertEqual(chipped, styles.body.lineHeight, accuracy: 0.5)
        XCTAssertEqual(try measured(m, "see plain now"), chipped, accuracy: 0.5)
        // The chip is wider than its text and sits centred in the line.
        let runs = TextShaper.runs(blocks[0].inlines, styles.body, mono: nil, ink: .black, link: .black, codeBackground: nil, chip: styles.chip, wrap: width)
        let rects = TextShaper.chipRects(in: runs, lineHeight: styles.body.lineHeight, wrap: width)
        XCTAssertEqual(rects.count, 1)
        let rect = try XCTUnwrap(rects.first?.rect)
        // #VAPP-258: the theme's rules for the chip's kind (`Markdown/chip*`) first.
        let k = styles.chip.resolved("issue")
        let label = TextShaper.width("ABC-1 Fix login", TextShaper.chipLabelStyle(k))
        XCTAssertEqual(rect.width, 2 * k.paddingH + k.icon + k.gap + label, accuracy: 0.01)
        XCTAssertEqual(rect.height, min(styles.chip.height, styles.body.lineHeight))
        XCTAssertEqual(rect.midY, styles.body.lineHeight / 2, accuracy: 0.01)
        XCTAssertGreaterThan(rect.minX, 0)
        XCTAssertEqual(rects.first?.run.href, "https://app.example/i/ABC-1")
        // A tap on the chip finds its link.
        XCTAssertEqual(TextShaper.link(in: runs, lineHeight: styles.body.lineHeight, wrap: width, at: CGPoint(x: rect.midX, y: rect.midY)), "https://app.example/i/ABC-1")
        // Chips that wrap: the measurer counts their widths (more lines than
        // the plain text), and still equals the painter's layout.
        let many = Array(repeating: "#ABC-1", count: 6).joined(separator: " ")
        let wrapped = try measured(m, many, width: 200)
        XCTAssertEqual(wrapped, try laidOut(m, many).0.height, accuracy: 0.5)
        XCTAssertGreaterThan(wrapped, try measured(m, Array(repeating: "#XYZ-1", count: 6).joined(separator: " "), width: 200))
    }

    func testImageSizeChangesTheMeasuredHeight() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        let host = FakeHost(f)
        let m = try model(host)
        // 1600×800 in 320: the fixed 160 (capped at once); a banner is shorter.
        let fixed = try measured(m, "![x](https://app.example/api/attachments/unknown.png)")
        XCTAssertEqual(fixed, 160, accuracy: 0.5)
        let sized = try measured(m, "![wide](https://app.example/api/attachments/wide.png)", width: 200)
        XCTAssertEqual(sized, 100, accuracy: 0.5)
        let (laid, _, _, _) = try laidOut(m, "![wide](https://app.example/api/attachments/wide.png)")
        XCTAssertEqual(laid.blocks[0].width ?? 0, 200, accuracy: 0.5)
        XCTAssertEqual(laid.height, sized, accuracy: 0.5)
    }

    func testOnImagePressWiring() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        let host = FakeHost(f)
        let m = try model(host)
        XCTAssertNil(m.host.onImagePress, "no hook = not pressable")
        host.pressable = true
        m.host.onImagePress?("https://app.example/api/attachments/wide.png")
        XCTAssertEqual(host.pressed, ["https://app.example/api/attachments/wide.png"])
        // ExponentialHost: the policy closure, else the base plugin's.
        var policyPressed: [String] = []
        let eh = ExponentialHost(HostOptions(policy: HostPolicy(onImagePress: { policyPressed.append($0) })))
        eh.plugin.onImagePress?("a.png")
        XCTAssertEqual(policyPressed, ["a.png"])
        XCTAssertNil(ExponentialHost(HostOptions()).plugin.onImagePress)
        let bridged = ExponentialHost(HostOptions(policy: HostPolicy(imageSize: { _ in CGSize(width: 4, height: 2) }, attachment: { _ in MarkdownAttachmentInfo(kind: "file", name: "n") })))
        XCTAssertEqual(bridged.plugin.imageSize("x"), CGSize(width: 4, height: 2))
        XCTAssertEqual(bridged.plugin.attachment("x")?.name, "n")
    }

    func testAttachmentHeightsInTheMeasurer() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        let m = try model(FakeHost(f))
        let video = try measured(m, "[clip](https://app.example/api/attachments/clip.mp4)", width: 390)
        XCTAssertEqual(video, 320 / 1.7777778, accuracy: 0.5)
        let narrow = try measured(m, "[clip](https://app.example/api/attachments/clip.mp4)", width: 200)
        XCTAssertEqual(narrow, 200 / 1.7777778, accuracy: 0.5)
        let audioText = "[voice note](https://app.example/api/attachments/voice.m4a)"
        let audio = try measured(m, audioText)
        let (laidAudio, styles, _, _) = try laidOut(m, audioText)
        XCTAssertEqual(audio, styles.body.lineHeight + m.spacing("xs") + m.control("row", 32), accuracy: 0.5)
        XCTAssertEqual(audio, laidAudio.height, accuracy: 0.5)
        let file = try measured(m, "[the report](https://app.example/api/attachments/report.pdf)")
        XCTAssertEqual(file, max(styles.body.lineHeight, m.control("iconSm", 16)), accuracy: 0.5)
        // Between paragraphs: the gaps of any block.
        let between = "before\n\n[the report](https://app.example/api/attachments/report.pdf)\n\nafter"
        XCTAssertEqual(try measured(m, between), try laidOut(m, between).0.height, accuracy: 0.5)
    }

    /// Paints a document with every hook without failing (the measured box holds).
    func testPaintsEveryHook() throws {
        guard Fixtures.available() else { return }
        let (f, _) = try fixture()
        let host = FakeHost(f)
        host.pressable = true
        let m = try model(host)
        let text = "see #ABC-1 and @ann@example.com\n\n| Issue |\n| --- |\n| #ABC-1 |\n\n![wide](https://app.example/api/attachments/wide.png)\n\n[clip](https://app.example/api/attachments/clip.mp4)\n\n[voice](https://app.example/api/attachments/voice.m4a)\n\n[r](https://app.example/api/attachments/report.pdf)"
        let h = try measured(m, text)
        XCTAssertEqual(h, try laidOut(m, text).0.height, accuracy: 0.5)
        var painted = Set<Int>()
        m.paintProbe = { painted.insert($0) }
        defer { m.paintProbe = nil }
        let root = ExponentialSurface(model: m).frame(width: 320)
        #if canImport(AppKit)
        let view = NSHostingView(rootView: root)
        view.frame = CGRect(x: 0, y: 0, width: 320, height: max(m.surfaceSize.height, 900))
        view.layoutSubtreeIfNeeded()
        _ = view.fittingSize
        #else
        renderInWindow(root, size: CGSize(width: 320, height: max(m.surfaceSize.height, 900)))
        #endif
        XCTAssertFalse(painted.isEmpty)
        XCTAssertTrue(m.paintFailures.isEmpty)
    }
}
