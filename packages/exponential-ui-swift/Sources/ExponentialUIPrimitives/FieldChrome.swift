import SwiftUI

/// The box a text field or a trigger sits in: fill, hairline, radius, with
/// a focus ring. The field itself (a UIKit/AppKit text view in the painter,
/// a SwiftUI `TextField` in an app) is the content.
public struct FieldChrome: ViewModifier {
    public var fill: Color
    public var stroke: Color
    public var focusedStroke: Color
    public var strokeWidth: CGFloat
    public var radius: CGFloat
    public var focused: Bool
    /// Stroke the hairline on the box's edge (SwiftUI `stroke`, half
    /// outside) instead of inside it (`strokeBorder`, the default).
    public var strokeCentered: Bool

    public init(fill: Color, stroke: Color, focusedStroke: Color, strokeWidth: CGFloat = 1, radius: CGFloat = 6, focused: Bool = false, strokeCentered: Bool = false) {
        self.fill = fill
        self.stroke = stroke
        self.focusedStroke = focusedStroke
        self.strokeWidth = strokeWidth
        self.radius = radius
        self.focused = focused
        self.strokeCentered = strokeCentered
    }

    public func body(content: Content) -> some View {
        content
            .background(fill, in: RoundedRectangle(cornerRadius: radius))
            .overlay {
                if strokeCentered {
                    RoundedRectangle(cornerRadius: radius).stroke(focused ? focusedStroke : stroke, lineWidth: strokeWidth)
                } else {
                    RoundedRectangle(cornerRadius: radius).strokeBorder(focused ? focusedStroke : stroke, lineWidth: strokeWidth)
                }
            }
    }
}

public extension View {
    func fieldChrome(fill: Color, stroke: Color, focusedStroke: Color, strokeWidth: CGFloat = 1, radius: CGFloat = 6, focused: Bool = false, strokeCentered: Bool = false) -> some View {
        modifier(FieldChrome(fill: fill, stroke: stroke, focusedStroke: focusedStroke, strokeWidth: strokeWidth, radius: radius, focused: focused, strokeCentered: strokeCentered))
    }
}

/// A drawn switch: a track with a travelling thumb, sized by the style (the
/// `Switch/track` + `thumb` recipes in the painter; the app's glass toggle).
/// Use it as a `ToggleStyle` on a SwiftUI `Toggle`.
public struct DrawnSwitchStyle: ToggleStyle {
    public var trackWidth: CGFloat
    public var trackHeight: CGFloat
    public var thumbSize: CGFloat
    public var trackOn: Color
    public var trackOff: Color
    public var thumb: Color
    public var thumbOn: Color?
    public var label: Color
    public var labelFont: Font
    public var spacing: CGFloat
    public var labelLeading: Bool
    /// The track hairline while off / on (nil = none).
    public var trackStroke: Color?
    public var trackStrokeOn: Color?
    public var trackStrokeWidth: CGFloat
    /// Stroke the track hairline on the capsule's edge (SwiftUI `stroke`)
    /// instead of inside it (`strokeBorder`, the default).
    public var trackStrokeCentered: Bool
    /// The track's own animation on a value change. nil = none, for a host
    /// that animates the toggle itself (`withAnimation`): a nil `.animation`
    /// would otherwise cancel the host's transaction.
    public var animation: Animation?

    public init(
        trackWidth: CGFloat = 32,
        trackHeight: CGFloat = 20,
        thumbSize: CGFloat = 16,
        trackOn: Color = .accentColor,
        trackOff: Color = Color.gray.opacity(0.35),
        thumb: Color = .white,
        thumbOn: Color? = nil,
        label: Color = .primary,
        labelFont: Font = .body,
        spacing: CGFloat = 8,
        labelLeading: Bool = true,
        trackStroke: Color? = nil,
        trackStrokeOn: Color? = nil,
        trackStrokeWidth: CGFloat = 1,
        trackStrokeCentered: Bool = false,
        animation: Animation? = .easeInOut(duration: 0.15)
    ) {
        self.trackWidth = trackWidth
        self.trackHeight = trackHeight
        self.thumbSize = thumbSize
        self.trackOn = trackOn
        self.trackOff = trackOff
        self.thumb = thumb
        self.thumbOn = thumbOn
        self.label = label
        self.labelFont = labelFont
        self.spacing = spacing
        self.labelLeading = labelLeading
        self.trackStroke = trackStroke
        self.trackStrokeOn = trackStrokeOn
        self.trackStrokeWidth = trackStrokeWidth
        self.trackStrokeCentered = trackStrokeCentered
        self.animation = animation
    }

    public func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: spacing) {
            if labelLeading {
                configuration.label.font(labelFont).foregroundStyle(label)
                Spacer(minLength: 0)
            }
            track(isOn: configuration.isOn)
                .onTapGesture { configuration.isOn.toggle() }
            if !labelLeading {
                configuration.label.font(labelFont).foregroundStyle(label)
            }
        }
    }

    /// The track alone (what the painter places in the `.track` frame).
    public func track(isOn: Bool) -> some View {
        let inset = max(0, (trackHeight - thumbSize) / 2)
        let hairline = isOn ? trackStrokeOn : trackStroke
        return ZStack(alignment: .leading) {
            Capsule().fill(isOn ? trackOn : trackOff)
            if let hairline {
                if trackStrokeCentered {
                    Capsule().stroke(hairline, lineWidth: trackStrokeWidth)
                } else {
                    Capsule().strokeBorder(hairline, lineWidth: trackStrokeWidth)
                }
            }
            Circle()
                .fill(isOn ? (thumbOn ?? thumb) : thumb)
                .frame(width: thumbSize, height: thumbSize)
                .offset(x: isOn ? trackWidth - thumbSize - inset : inset)
        }
        .frame(width: trackWidth, height: trackHeight)
        .modifier(TrackAnimation(animation: animation, isOn: isOn))
        .accessibilityHidden(true)
    }
}

private struct TrackAnimation: ViewModifier {
    let animation: Animation?
    let isOn: Bool

    func body(content: Content) -> some View {
        if let animation {
            content.animation(animation, value: isOn)
        } else {
            content
        }
    }
}
