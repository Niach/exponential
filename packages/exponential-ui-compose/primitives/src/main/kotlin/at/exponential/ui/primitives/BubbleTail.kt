package at.exponential.ui.primitives

import androidx.compose.ui.graphics.Path
import kotlin.math.max
import kotlin.math.sqrt

/**
 * #EXP-1876: catalog/style.json `tail`, the speech-bubble tail ("Zipfel")
 * painted with its box as ONE outline (the renderer's Box and the app's
 * message bubble share these numbers). PHYSICAL sides: [Left] / [Right] =
 * at the top corner of that side (that corner square), [Down] = under the
 * bottom edge near the right, [DownLeft] its mirror.
 */
enum class BubbleTail {
    Left, Right, Down, DownLeft;

    /** A side tail squares this corner `[topLeft, topRight, bottomRight, bottomLeft]` (null = none). */
    val squaredCorner: Int? get() = when (this) {
        Left -> 0
        Right -> 1
        else -> null
    }

    companion object {
        /** The logical `start | end | down` of the style key, on the physical side ([rtl] mirrors). */
        fun of(tail: String?, rtl: Boolean): BubbleTail? = when (tail) {
            "start" -> if (rtl) Right else Left
            "end" -> if (rtl) Left else Right
            "down" -> if (rtl) DownLeft else Down
            else -> null
        }
    }
}

/** The tail's numbers (dp): 6 out, 9 along the side, a ~2 rounded tip, a down tail's vertical edge 22 in from the end. */
object BubbleTailGeometry {
    const val OUT = 6f
    const val SIDE = 9f
    const val TIP = 2f
    const val DOWN_INSET = 22f

    /** The hypotenuse from the tip (-6, 0) to the side point (0, 9). */
    val HYPOTENUSE = sqrt(OUT * OUT + SIDE * SIDE)

    /** [corners] with the tail's corner square (a down tail keeps every radius). */
    fun corners(corners: List<Float>, tail: BubbleTail?): List<Float> {
        val i = tail?.squaredCorner ?: return corners
        return corners.toMutableList().also { it[i] = 0f }
    }
}

/** One step of an outline (dp, the box's frame at (0, 0)). */
sealed class OutlineStep {
    data class Move(val x: Float, val y: Float) : OutlineStep()
    data class Line(val x: Float, val y: Float) : OutlineStep()
    data class Quad(val cx: Float, val cy: Float, val x: Float, val y: Float) : OutlineStep()
    data class Cubic(val x1: Float, val y1: Float, val x2: Float, val y2: Float, val x: Float, val y: Float) : OutlineStep()
    data object Close : OutlineStep()
}

/**
 * The ONE outline of a [width] × [height] box (dp) with its [corners]
 * `[topLeft, topRight, bottomRight, bottomLeft]` (already clamped; the
 * tail's corner is squared here) and its [tail], inset by [inset] dp (a
 * border strokes it inset by half its width, inside like CSS). Clockwise
 * from the tail's tip; the tail lies OUTSIDE the frame (paint-only).
 */
fun bubbleTailOutline(width: Float, height: Float, corners: List<Float>, tail: BubbleTail, inset: Float = 0f): List<OutlineStep> {
    val mirrored = tail == BubbleTail.Right || tail == BubbleTail.DownLeft
    val side = tail == BubbleTail.Left || tail == BubbleTail.Right
    val sq = BubbleTailGeometry.corners(corners, tail)
    // Built in the Left / Down orientation; a mirrored tail swaps the corners and flips x.
    val c = if (mirrored) listOf(sq[1], sq[0], sq[3], sq[2]) else sq
    val d = inset
    val r = c.map { max(0f, it - d) }
    val k = 0.5523f
    val len = BubbleTailGeometry.HYPOTENUSE
    val out = BubbleTailGeometry.OUT
    val dn = BubbleTailGeometry.SIDE
    val tip = BubbleTailGeometry.TIP
    val ux = out / len
    val uy = dn / len
    val w = width
    val h = height
    val steps = ArrayList<OutlineStep>(16)
    fun x(v: Float) = if (mirrored) w - v else v
    fun move(a: Float, b: Float) = steps.add(OutlineStep.Move(x(a), b))
    fun line(a: Float, b: Float) = steps.add(OutlineStep.Line(x(a), b))
    fun quad(cx: Float, cy: Float, a: Float, b: Float) = steps.add(OutlineStep.Quad(x(cx), cy, x(a), b))
    fun cubic(x1: Float, y1: Float, x2: Float, y2: Float, a: Float, b: Float) = steps.add(OutlineStep.Cubic(x(x1), y1, x(x2), y2, x(a), b))
    val left = d
    val top = d
    val right = w - d
    val bottom = h - d
    // The side tail's hypotenuse 9x - 6y + 54 = 0, shifted inward by d,
    // crosses y = d at x = -6 + d·(len + 6) / 9 (the inset tip).
    val tipX = -out + d * (len + out) / dn
    val tipY = d
    if (side) move(tipX + tip, tipY) else move(left + r[0], top)
    line(right - r[1], top)
    if (r[1] > 0f) cubic(right - r[1] + k * r[1], top, right, top + r[1] - k * r[1], right, top + r[1])
    line(right, bottom - r[2])
    if (r[2] > 0f) cubic(right, bottom - r[2] + k * r[2], right - r[2] + k * r[2], bottom, right - r[2], bottom)
    if (!side) {
        // Straight down at w - 22 to the tip (w - 22, h + 9), the hypotenuse back
        // up to (w - 28, h). Shifted by d (u, v from (w - 22, h)): the
        // vertical at u = -d meets it at v = 9 - d·(len + 9) / 6, the bottom
        // edge v = -d at u = -6 + d·(len - 6) / 9.
        val vx = w - BubbleTailGeometry.DOWN_INSET - d
        val ty = h + dn - d * (len + dn) / out
        line(vx, bottom)
        line(vx, ty - tip)
        quad(vx, ty, vx - tip * ux, ty - tip * uy)
        line(w - BubbleTailGeometry.DOWN_INSET - out + d * (len - out) / dn, bottom)
    }
    line(left + r[3], bottom)
    if (r[3] > 0f) cubic(left + r[3] - k * r[3], bottom, left, bottom - r[3] + k * r[3], left, bottom - r[3])
    if (side) {
        // Up the side to where the (shifted) hypotenuse meets it, out to the rounded tip.
        line(left, dn - d * (len - dn) / out)
        line(tipX + tip * ux, tipY + tip * uy)
        quad(tipX, tipY, tipX + tip, tipY)
    } else {
        line(left, top + r[0])
        if (r[0] > 0f) cubic(left, top + r[0] - k * r[0], left + r[0] - k * r[0], top, left + r[0], top)
    }
    steps.add(OutlineStep.Close)
    return steps
}

/** [bubbleTailOutline] as a Compose [Path] in px ([density] px per dp, [inset] in px). */
fun bubbleTailPath(width: Float, height: Float, corners: List<Float>, tail: BubbleTail, density: Float, inset: Float = 0f): Path {
    val path = Path()
    for (s in bubbleTailOutline(width, height, corners, tail, inset / density)) when (s) {
        is OutlineStep.Move -> path.moveTo(s.x * density, s.y * density)
        is OutlineStep.Line -> path.lineTo(s.x * density, s.y * density)
        is OutlineStep.Quad -> path.quadraticTo(s.cx * density, s.cy * density, s.x * density, s.y * density)
        is OutlineStep.Cubic -> path.cubicTo(s.x1 * density, s.y1 * density, s.x2 * density, s.y2 * density, s.x * density, s.y * density)
        OutlineStep.Close -> path.close()
    }
    return path
}
