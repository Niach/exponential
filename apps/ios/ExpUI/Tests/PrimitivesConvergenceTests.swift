import SwiftUI
import XCTest
import ExpCore
import ExponentialUIPrimitives
@testable import ExpUI

// SLOP-18 / VAPP-88: ExpUI's glass views are thin wrappers over the Exponential
// UI SDK's `ExponentialUIPrimitives`. The wrappers hand their pinned geometry
// INTO the primitives' style structs, so a converged view must carry exactly
// the numbers it drew with before (no app screen may move a point).
final class PrimitivesConvergenceTests: XCTestCase {

    // MARK: - Pill

    func testPillRungsAreUnchanged() {
        XCTAssertEqual(GlassPillTokens.heightSm, 24)
        XCTAssertEqual(GlassPillTokens.heightMd, 32)
        XCTAssertEqual(GlassPillTokens.horizontalPaddingSm, 8)
        XCTAssertEqual(GlassPillTokens.horizontalPaddingMd, 12)
        XCTAssertEqual(GlassPillTokens.dotSize, 6)
    }

    func testPillStyleCarriesTheRungGeometry() {
        let sm = GlassPillSize.sm.pillStyle(paint: GlassPillPaint(), label: .white)
        XCTAssertEqual(sm.height, GlassPillTokens.heightSm)
        XCTAssertEqual(sm.horizontalPadding, GlassPillTokens.horizontalPaddingSm)
        XCTAssertEqual(sm.spacing, GlassPillTokens.spacingSm)
        XCTAssertEqual(sm.glyphSize, GlassPillTokens.glyphSm)
        XCTAssertEqual(sm.dotSize, GlassPillTokens.dotSize)
        XCTAssertEqual(sm.font, .caption.weight(.medium))

        let md = GlassPillSize.md.pillStyle(paint: GlassPillPaint(), label: .white)
        XCTAssertEqual(md.height, GlassPillTokens.heightMd)
        XCTAssertEqual(md.horizontalPadding, GlassPillTokens.horizontalPaddingMd)
        XCTAssertEqual(md.spacing, GlassPillTokens.spacingMd)
        XCTAssertEqual(md.glyphSize, GlassPillTokens.glyphMd)
        XCTAssertEqual(md.font, .subheadline.weight(.medium))
    }

    func testPillPaintsMatchTheGlassButtonChrome() {
        let rest = GlassPillSize.sm.pillStyle(paint: GlassPillPaint(), label: .white)
        XCTAssertEqual(rest.fill, GlassTokens.fillCard)
        XCTAssertEqual(rest.stroke, GlassTokens.strokeCard)
        XCTAssertEqual(rest.strokeWidth, GlassTokens.hairline)
        XCTAssertTrue(rest.strokeCentered)
        XCTAssertNil(rest.underlay)

        let selected = GlassPillSize.sm.pillStyle(paint: GlassPillPaint(isSelected: true, isOpaque: true), label: .white)
        XCTAssertEqual(selected.fill, GlassTokens.fillActive)
        XCTAssertEqual(selected.stroke, GlassTokens.strokeActive)
        XCTAssertEqual(selected.underlay, DesignTokens.Palette.card)

        let primary = GlassPillSize.md.pillStyle(paint: GlassPillPaint(isPrimary: true, isOpaque: true), label: .white)
        XCTAssertEqual(primary.fill, DesignTokens.Palette.primary)
        XCTAssertNil(primary.stroke)
        XCTAssertNil(primary.underlay)

        let toned = GlassPillSize.sm.pillStyle(paint: GlassPillPaint(tint: .red), label: .red)
        XCTAssertEqual(toned.fill, GlassTokens.fillCard)
        XCTAssertEqual(toned.stroke, Color.red.opacity(0.4))
    }

    // MARK: - Segmented control

    func testSegmentedStyleCarriesTheStripTokens() {
        let capsule = GlassSegmentedControl<Int>.segmentedStyle(.capsule)
        XCTAssertEqual(capsule.height, GlassSegmentedControlTokens.height)
        XCTAssertEqual(capsule.inset, GlassSegmentedControlTokens.capsulePadding)
        XCTAssertEqual(capsule.segmentVerticalPadding, GlassSegmentedControlTokens.segmentVerticalPadding)
        XCTAssertEqual(capsule.trackFill, GlassSegmentedControlTokens.containerFill)
        XCTAssertEqual(capsule.trackStroke, GlassSegmentedControlTokens.stroke)
        XCTAssertEqual(capsule.segmentFill, GlassSegmentedControlTokens.activeFill)
        XCTAssertEqual(capsule.horizontalPadding, 0)
        XCTAssertTrue(capsule.capsule)
        XCTAssertTrue(capsule.minimumHeight)
        XCTAssertTrue(capsule.showsTrack)

        XCTAssertFalse(GlassSegmentedControl<Int>.segmentedStyle(.embedded).showsTrack)
    }

    // MARK: - Switch

    func testSwitchKeepsTheUISwitchGeometry() {
        let drawn = GlassToggleStyle.drawn
        XCTAssertEqual(drawn.trackWidth, 51)
        XCTAssertEqual(drawn.trackHeight, 31)
        XCTAssertEqual(drawn.thumbSize, 27)
        XCTAssertEqual((drawn.trackHeight - drawn.thumbSize) / 2, 2)
        XCTAssertEqual(drawn.trackOn, DesignTokens.Palette.primary)
        XCTAssertEqual(drawn.trackOff, GlassTokens.fillCard)
        XCTAssertEqual(drawn.thumbOn, DesignTokens.Palette.primaryForeground)
        XCTAssertEqual(drawn.thumb, DesignTokens.Palette.mutedForeground)
        XCTAssertEqual(drawn.trackStroke, GlassTokens.strokeCard)
        XCTAssertNil(drawn.trackStrokeOn)
        XCTAssertNil(drawn.animation)
    }

    // MARK: - Avatar, meter

    func testUserAvatarKeepsItsOwnHue() {
        XCTAssertEqual(UserAvatarTokens.initialsScale, 0.42)
        XCTAssertEqual(UserAvatarTokens.minimumScaleFactor, 0.6)
        XCTAssertEqual(UserAvatar.ink("user-1"), DesignTokens.Avatar.hues[avatarHueIndex("user-1")])
        XCTAssertEqual(UserAvatar.fill("user-1"), DesignTokens.Avatar.hues[avatarHueIndex("user-1")].opacity(0.2))
    }

    func testUsageTrackFractionClamps() {
        XCTAssertEqual(AgentUsageTrack.fraction(nil), 0)
        XCTAssertEqual(AgentUsageTrack.fraction(-5), 0)
        XCTAssertEqual(AgentUsageTrack.fraction(42), 0.42, accuracy: 1e-9)
        XCTAssertEqual(AgentUsageTrack.fraction(140), 1)
        XCTAssertEqual(MeterTrack.width(0.5, 200), 100)
        XCTAssertEqual(MeterTrack.width(1.5, 200), 200)
    }
}
