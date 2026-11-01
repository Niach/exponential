import XCTest
import SwiftUI
import ImageIO
import ExponentialUICore
@testable import ExponentialUI

/// EXP-1604: Motion over the FFI. The shared clock replays
/// `fixtures/motion-playback.json`; the painter paints every case of
/// `fixtures/motion-frames.json` at 160×160 and matches the raster's
/// `conformance/motion/parity/<id>@<frame>.png`: ≥ 99 % of pixels within
/// 2/255 per channel or inside the reference's 3×3 range (`compare`); the
/// strict share is printed per case.
@MainActor
final class MotionTests: XCTestCase {
    func testPlaybackFixtureReplays() throws {
        try XCTSkipUnless(Fixtures.available())
        let f = try Fixtures.json("motion-playback.json")
        let lottie = f["lottie"]!.json
        for c in f["cases"]!.array! {
            let id = c["id"]!.string!
            let player = try MotionPlayer(lottieJson: lottie, slotsJson: "")
            player.setPlayback(playbackJson: c["props"]!.json)
            XCTAssertEqual(player.posterFrame(), c["poster"]!.number!, accuracy: 1e-6, "\(id) poster")
            XCTAssertEqual(player.progressFrame(progress: 0.5), c["progressHalf"]!.number!, accuracy: 1e-6, "\(id) progress")
            for t in c["ticks"]!.array! {
                let got = player.tick(elapsedMs: t["elapsedMs"]!.number!)
                XCTAssertEqual(got.frame, t["frame"]!.number!, accuracy: 1e-6, "\(id) @\(t["elapsedMs"]!.number!)")
                XCTAssertEqual(got.done, t["done"]!.bool!, "\(id) done @\(t["elapsedMs"]!.number!)")
            }
        }
    }

    func testPackedFramesDecodeLikeTheirLists() throws {
        try XCTSkipUnless(Fixtures.available())
        for c in try Fixtures.json("motion-frames.json")["cases"]!.array! {
            for f in c["frames"]!.array! {
                let packed = f["packed"]!.array!.map { Float($0.number!) }
                let list = try XCTUnwrap(MotionList(packed: packed), c["id"]!.string!)
                XCTAssertEqual(list.ops.count, f["list"]!["ops"]!.array!.count, c["id"]!.string!)
                // The FFI samples the same stream the fixture pins.
                let player = try MotionPlayer(lottieJson: c["lottie"]!.json, slotsJson: "")
                XCTAssertEqual(player.sample(frame: f["frame"]!.number!), packed, c["id"]!.string!)
            }
        }
    }

    func testPixelParityWithTheRaster() throws {
        try XCTSkipUnless(Fixtures.available())
        let parity = Fixtures.dir.deletingLastPathComponent().appendingPathComponent("conformance/motion/parity")
        var report: [String] = []
        var failed: [String] = []
        for c in try Fixtures.json("motion-frames.json")["cases"]!.array! {
            let id = c["id"]!.string!
            let player = try MotionPlayer(lottieJson: c["lottie"]!.json, slotsJson: "")
            let images = player.images().map(MotionPainter.decodeDataURL)
            for f in c["frames"]!.array! {
                let frame = f["frame"]!.number!
                let name = "\(id)@\(Int(frame))"
                let list = try XCTUnwrap(MotionList(packed: player.sample(frame: frame)))
                let got = try XCTUnwrap(Self.paint(list, images: images), name)
                let want = try XCTUnwrap(Self.png(parity.appendingPathComponent("\(name).png")), name)
                let (share, worst, edgeAware) = Self.compare(got, want)
                report.append("\(name): \(String(format: "%.2f", share * 100)) % within 2/255 (worst \(worst)), \(String(format: "%.2f", edgeAware * 100)) % edge-aware")
                if edgeAware < 0.99 { failed.append(name) }
                // EXPONENTIAL_UI_MOTION_OUT=<dir>: every paint as a PNG, to eyeball against the reference.
                if let dir = ProcessInfo.processInfo.environment["EXPONENTIAL_UI_MOTION_OUT"] {
                    Self.write(got, to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
                }
            }
        }
        print("Motion parity (SwiftUI vs raster):\n" + report.joined(separator: "\n"))
        XCTAssert(failed.isEmpty, "Motion parity under 99 %: \(failed)\n" + report.joined(separator: "\n"))
    }

    func testThemeSlotsAndModeReachTheFile() throws {
        let lottie = #"{"slots":{"$color.primary":{"p":{"a":0,"k":[0.5,0.5,0.5,1]}},"ld:#ffffff:#000000":{}}}"#
        let json = MotionSource.slots(lottie: lottie, props: ["slots": .object(["x": .number(1)])], color: { $0 == "primary" ? "#ff0000" : nil }, dark: true)
        let v = JSONValue.parse(json)
        XCTAssertEqual(v["$color.primary"]?["p"]?["k"]?.array?.compactMap(\.number), [1, 0, 0, 1])
        XCTAssertEqual(v["$mode"]?.string, "dark")
        XCTAssertEqual(v["x"]?.number, 1)
        XCTAssertNil(v["ld:#ffffff:#000000"])
    }

    func testInlineLayersCompileThroughTheCore() throws {
        let props = JSONValue.parse(#"{"size":[48,48],"duration":1000,"layers":[{"type":"shape","shapes":[{"type":"ellipse","size":[36,36]},{"type":"fill","color":"$color.primary"}]}],"label":"x"}"#).object!
        let lottie = try XCTUnwrap(MotionSource.inlineLottie(props))
        XCTAssertNotNil(JSONValue.parse(lottie)["slots"]?["$color.primary"])
        let player = try MotionPlayer(lottieJson: lottie, slotsJson: "")
        XCTAssertEqual(player.size(), [48, 48])
    }

    func testAtMostEightAnimatePerSurface() throws {
        let m = try makeModel()
        let one = #"{"component":"Motion","props":{"size":[10,10],"duration":100,"layers":[]}}"#
        let kids = (0..<10).map { i in one.replacingOccurrences(of: #"{"component""#, with: #"{"id":"m\#(i)","component""#) }
        try m.setNested(json: #"{"id":"col","component":"Stack","children":[\#(kids.joined(separator: ","))]}"#)
        let motions = m.nodes.filter { $0.component == "Motion" }
        XCTAssertEqual(motions.count, 10)
        XCTAssertEqual(motions.map { m.motionMayAnimate($0.index) }, Array(repeating: true, count: 8) + [false, false])
    }

    // MARK: - Pixels

    static func paint(_ list: MotionList, images: [CGImage?]) -> CGImage? {
        let view = Canvas { ctx, size in
            MotionPainter.paint(list, in: &ctx, box: size, images: images)
        }
        .frame(width: 160, height: 160)
        let r = ImageRenderer(content: view)
        r.scale = 1
        r.isOpaque = false
        return r.cgImage
    }

    static func png(_ url: URL) -> CGImage? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(src, 0, nil)
    }

    /// Premultiplied sRGB RGBA8 pixels of `img` at 160×160.
    static func pixels(_ img: CGImage) -> [UInt8] {
        var buf = [UInt8](repeating: 0, count: 160 * 160 * 4)
        buf.withUnsafeMutableBytes { p in
            let ctx = CGContext(data: p.baseAddress, width: 160, height: 160, bitsPerComponent: 8, bytesPerRow: 640,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            ctx.interpolationQuality = .none
            ctx.draw(img, in: CGRect(x: 0, y: 0, width: 160, height: 160))
        }
        return buf
    }

    /// The share of pixels whose every channel is within 2/255, the worst
    /// delta, and the EDGE-AWARE share: a pixel also passes when every
    /// channel lies within the reference's 3×3 neighbourhood range ± 2. Core
    /// Graphics computes exact edge coverage, the raster 4× supersampled
    /// (coverage in steps of 1/16), so stroke-heavy frames differ on most
    /// edge pixels; the edge-aware share holds the geometry to the raster.
    static func compare(_ a: CGImage, _ b: CGImage) -> (Double, Int, Double) {
        let pa = pixels(a), pb = pixels(b)
        var ok = 0, near = 0, worst = 0
        for y in 0..<160 {
            for x in 0..<160 {
                let i = (y * 160 + x) * 4
                var d = 0
                for c in 0..<4 { d = max(d, abs(Int(pa[i + c]) - Int(pb[i + c]))) }
                worst = max(worst, d)
                if d <= 2 { ok += 1; near += 1; continue }
                var inside = true
                for c in 0..<4 {
                    var lo = 255, hi = 0
                    for dy in -1...1 { for dx in -1...1 {
                        let xx = min(159, max(0, x + dx)), yy = min(159, max(0, y + dy))
                        let v = Int(pb[(yy * 160 + xx) * 4 + c])
                        lo = min(lo, v); hi = max(hi, v)
                    } }
                    let v = Int(pa[i + c])
                    if v < lo - 2 || v > hi + 2 { inside = false; break }
                }
                if inside { near += 1 }
            }
        }
        let n = Double(pa.count / 4)
        return (Double(ok) / n, worst, Double(near) / n)
    }

    static func write(_ img: CGImage, to url: URL) {
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil) else { return }
        CGImageDestinationAddImage(dest, img, nil)
        CGImageDestinationFinalize(dest)
    }
}
