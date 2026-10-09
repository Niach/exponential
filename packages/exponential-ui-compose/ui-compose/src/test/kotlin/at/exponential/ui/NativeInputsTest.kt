package at.exponential.ui

import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.autofill.AutofillType
import androidx.compose.ui.text.input.KeyboardType
import at.exponential.ui.compose.autoCorrectOf
import at.exponential.ui.compose.autofillTypesOf
import at.exponential.ui.compose.keyboardTypeOf
import at.exponential.ui.compose.sliderRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-103: the platform Slider keeps the author's range and step, and an
 * Input `type` drives the keyboard, autocorrection and autofill hints.
 */
class NativeInputsTest {
    @Test
    fun sliderKeepsARangeNarrowerThanOne() {
        val g = sliderRange(0.0, 0.5, 0.1)
        assertEquals(0f, g.range.start)
        assertEquals(0.5f, g.range.endInclusive)
        // 0, 0.1, …, 0.5 = 6 stops = 4 steps between the ends.
        assertEquals(4, g.steps)
    }

    @Test
    fun sliderWithoutAStepIsContinuous() {
        assertEquals(0, sliderRange(0.0, 1.0, 0.0).steps)
        assertEquals(0, sliderRange(0.0, 1.0, -1.0).steps)
    }

    @Test
    fun sliderStepsCountTheStopsBetweenTheEnds() {
        assertEquals(99, sliderRange(0.0, 100.0, 1.0).steps)
        assertEquals(3, sliderRange(10.0, 50.0, 10.0).steps)
        // A whole-range step: two stops, nothing between.
        assertEquals(0, sliderRange(0.0, 5.0, 5.0).steps)
        // A step that does not divide the range stays continuous (snap does the rest).
        assertEquals(0, sliderRange(0.0, 10.0, 3.0).steps)
        // Too many stops for the platform's tick array: continuous + snap.
        assertEquals(0, sliderRange(0.0, 1_000_000.0, 1.0).steps)
    }

    @Test
    fun onlyAnEmptyOrInvertedRangeWidens() {
        assertEquals(5f..6f, sliderRange(5.0, 5.0, 1.0).range)
        assertEquals(5f..6f, sliderRange(5.0, 2.0, 1.0).range)
        assertEquals(-1f..-0.5f, sliderRange(-1.0, -0.5, 0.0).range)
    }

    @Test
    fun inputTypeKeyboards() {
        assertEquals(KeyboardType.Email, keyboardTypeOf("email"))
        assertEquals(KeyboardType.Uri, keyboardTypeOf("url"))
        assertEquals(KeyboardType.Phone, keyboardTypeOf("tel"))
        assertEquals(KeyboardType.Password, keyboardTypeOf("password"))
        assertEquals(KeyboardType.Text, keyboardTypeOf("text"))
    }

    @Test
    fun addressesNumbersAndSecretsAreNotAutocorrected() {
        for (t in listOf("email", "url", "tel", "password", "number")) assertFalse(t, autoCorrectOf(t))
        for (t in listOf("text", "search", "")) assertTrue(t, autoCorrectOf(t))
    }

    @OptIn(ExperimentalComposeUiApi::class)
    @Test
    fun autofillHintsFollowTheType() {
        assertEquals(listOf(AutofillType.EmailAddress), autofillTypesOf("email"))
        assertEquals(listOf(AutofillType.Password), autofillTypesOf("password"))
        assertEquals(listOf(AutofillType.PhoneNumber), autofillTypesOf("tel"))
        assertTrue(autofillTypesOf("text").isEmpty())
        assertTrue(autofillTypesOf("url").isEmpty())
    }
}
