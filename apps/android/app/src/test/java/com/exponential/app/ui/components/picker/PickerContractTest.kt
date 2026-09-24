package com.exponential.app.ui.components.picker

import androidx.compose.ui.graphics.Color
import com.exponential.app.ui.theme.GlassTokens
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1029 contract, EXP-1021 implementation — the shared picker API on
 * Android. Web, the IDE and iOS carry the SAME case names; what differs is how
 * a case is decided, because Android's unit suite is plain JVM (no
 * Robolectric, CI runs `:app:testProductionDebugUnitTest`). So the behaviour
 * cases pin [PickerRules] / [PickerDefaults] — the pure decisions the
 * composable is a thin shell over — and the two structural cases read the
 * primitive's SOURCE, the way the icon and licence gates read theirs. A rule
 * that only exists inside a `@Composable` is a rule nothing on this platform
 * can hold, which is how the four sheets drifted apart in the first place.
 */
class PickerContractTest {

    @Test
    fun aRowMatchesOnItsKeywordsElseOnItsLabel() {
        assertEquals(listOf("Alpha"), PickerItem(value = "a", label = "Alpha").searchKeywords)
        assertEquals(
            listOf("APP-1"),
            PickerItem(value = "a", label = "Alpha", keywords = listOf("APP-1")).searchKeywords,
        )
    }

    @Test
    fun theTypedPickersMapTheirRowsToItems() {
        assertEquals(
            listOf("APP-1 Fix"),
            issuePickerItems(listOf(IssuePickerIssue("i", "APP-1", "Fix"))).map { it.label },
        )
        assertEquals(
            listOf("Unassigned", "Ada"),
            assigneePickerItems(listOf(AssigneePickerMember("u", "Ada")), allowsNone = true).map { it.label },
        )
        // The device set is the registry's append-only `devicePickable` list;
        // `laptop` has been in it since EXP-924 and can never leave.
        assertTrue(iconPickerItems(IconPickerSet.Device).any { it.value == "laptop" })
        assertTrue(iconPickerItems(IconPickerSet.Board).none { it.value == "laptop" })
    }

    /**
     * Every typed picker delegates to [Picker] and none of them draws a sheet
     * of its own — that delegation IS the "one surface" this issue asked for.
     * The icon picker's composable lives with the trigger it has always had
     * (`ui/components/IconPicker.kt`), so it is named here explicitly.
     */
    @Test
    fun everyTypedPickerRendersThroughThePrimitive() {
        val typed = listOf(
            "BoardPicker", "IssuePicker", "ActionPicker", "AccountPicker", "DevicePicker",
            "AssigneePicker", "StatusPicker", "PriorityPicker", "LabelPicker",
        )
        for (name in typed) {
            val source = pickerSource("$name.kt")
            assertTrue("$name must delegate to Picker(", source.contains("    Picker("))
            assertFalse("$name must not draw a sheet of its own", source.contains("GlassSheet("))
        }
        // The tenth: the icon picker keeps its swatch trigger and rides the
        // primitive's sheet as a panel.
        val iconPicker = moduleFile("src/main/java/com/exponential/app/ui/components/IconPicker.kt").readText()
        assertTrue(iconPicker.contains("Picker("))
        assertTrue(iconPicker.contains("panel = {"))
        assertFalse("the icon picker must not draw a sheet of its own", iconPicker.contains("GlassSheet("))
    }

    /**
     * A bottom sheet of PLAIN rows: the shared [GlassSheet] shell, and nothing
     * card-shaped between it and the list. No `glassCard`, no `glassRow`, no
     * `glassGroup` — the three card idioms the four old picker sheets used.
     */
    @Test
    fun aPhoneSheetOfPlainRowsNoCardsInsideTheSheet() {
        val source = pickerSource("Picker.kt")
        assertTrue("the picker IS the shared sheet", source.contains("GlassSheet(title = title"))
        for (card in listOf("glassCard(", "glassGroup(", "GlassCard(", "OptionGroup {")) {
            assertFalse("no $card inside the picker sheet", source.contains(card))
        }
        // The row's only paint is the EXP-818 flat row: transparent at rest.
        assertTrue(source.contains("flatRow(active = picked)"))
        assertEquals(Color.Transparent, PickerDefaults.rowBackground(picked = false))
    }

    /**
     * The selection language EXP-1021 asked for: a picked row reads as the
     * row's own highlight, never a leading circle or a trailing check.
     */
    @Test
    fun multiModeMarksPickedRowsByTheHighlightColourNeverACircle() {
        assertEquals(GlassTokens.RowFillActive, PickerDefaults.rowBackground(picked = true))
        assertEquals(Color.Transparent, PickerDefaults.rowBackground(picked = false))
        val source = pickerSource("Picker.kt")
        for (circle in listOf("uiCheck", "Checkbox", "RadioButton", "uiMinus")) {
            assertFalse("a picked row is a highlight, never a $circle", source.contains(circle))
        }
        // Toggling reports the WHOLE new set, so the caller never has to
        // reconstruct it from a single row.
        val items = items()
        val afterAdd = PickerRules.select(PickerMode.Multi, setOf("a"), items[1])
        assertEquals(setOf("a", "b"), afterAdd)
        assertEquals(emptySet<String>(), PickerRules.select(PickerMode.Multi, setOf("a"), items[0]))
    }

    @Test
    fun singleModeClosesOnAPickMultiModeStaysOpen() {
        assertTrue(PickerRules.closesOnPick(PickerMode.Single))
        assertFalse(PickerRules.closesOnPick(PickerMode.Multi))
        // A single pick REPLACES the selection rather than adding to it.
        assertEquals(setOf("b"), PickerRules.select(PickerMode.Single, setOf("a"), items()[1]))
    }

    @Test
    fun searchFiltersRowsByLabelAndKeywords() {
        val rows = listOf(
            PickerItem(value = "a", label = "Alpha", keywords = listOf("APP-1", "Alpha")),
            PickerItem(value = "b", label = "Beta", keywords = listOf("APP-2", "Beta")),
            PickerItem(value = "c", label = "Gamma"),
        )
        assertEquals(listOf("b"), PickerRules.filter(rows, "Bet").map { it.value })
        assertEquals(listOf("a"), PickerRules.filter(rows, "APP-1").map { it.value })
        // A row with no keywords of its own falls back to its label …
        assertEquals(listOf("c"), PickerRules.filter(rows, "gam").map { it.value })
        // … and a blank query leaves the caller's order untouched.
        assertEquals(rows.map { it.value }, PickerRules.filter(rows, "  ").map { it.value })
    }

    @Test
    fun aDisabledRowRendersButNeverPicks() {
        val disabled = PickerItem(value = "c", label = "Gamma", disabled = true)
        // It still matches search — it renders, it just cannot be chosen.
        assertEquals(listOf("c"), PickerRules.filter(listOf(disabled), "Gam").map { it.value })
        assertNull(PickerRules.select(PickerMode.Single, emptySet(), disabled))
        assertNull(PickerRules.select(PickerMode.Multi, emptySet(), disabled))
    }

    @Test
    fun emptyTextShowsForNoItemsAndForAnEmptySearch() {
        // ONE empty state, reached both ways: nothing to pick, and nothing
        // matched. The sheet renders `emptyText` whenever the rows run out.
        assertTrue(PickerRules.filter(emptyList<PickerItem<String>>(), "").isEmpty())
        assertTrue(PickerRules.filter(items(), "zzz").isEmpty())
        assertTrue(pickerSource("Picker.kt").contains("rows.isEmpty() && emptyText != null"))
    }

    /**
     * A colour WITH a glyph tints the glyph; a colour WITHOUT one draws the
     * row's dot (that is what makes a label row a dot and a board row a tinted
     * glyph without the caller choosing a shape), and a description is the
     * muted second line.
     */
    @Test
    fun aRowDrawsItsIconInItsColourAndItsDescriptionMuted() {
        val board = boardPickerItems(
            listOf(BoardPickerBoard(id = "b", name = "Mobile", icon = "rocket", colorHex = "#3B82F6")),
        ).single()
        assertTrue("a board row carries its glyph", board.icon != null)
        val label = labelPickerItems(listOf(LabelPickerLabel(id = "l", name = "bug", colorHex = "#EF4444")))
            .single()
        assertNull("a label row is a coloured DOT, never a glyph", label.icon)
        assertTrue(label.color != null)
        val member = assigneePickerItems(
            listOf(AssigneePickerMember(id = "u", name = "Ada", email = "ada@example.com")),
            allowsNone = false,
        ).single()
        assertEquals("ada@example.com", member.description)
        val source = pickerSource("Picker.kt")
        assertTrue(source.contains("PickerDefaults.DotSize"))
        assertTrue(source.contains("TextEmphasis.Tertiary"))
    }

    private fun items(): List<PickerItem<String>> = listOf(
        PickerItem(value = "a", label = "Alpha"),
        PickerItem(value = "b", label = "Beta", description = "second"),
        PickerItem(value = "c", label = "Gamma", disabled = true),
    )

    private fun pickerSource(name: String): String =
        moduleFile("src/main/java/com/exponential/app/ui/components/picker/$name").readText()

    /**
     * A module-relative source file, located the way [com.exponential.app.ui.emoji]
     * locates the emoji asset — Gradle runs the suite from the `app` module
     * dir, with fallbacks so it also runs from the repo root or `apps/android`.
     */
    private fun moduleFile(path: String): File =
        listOf(path, "app/$path", "apps/android/app/$path")
            .map(::File)
            .firstOrNull { it.isFile }
            ?: error("$path not found from ${File(".").absolutePath}")
}
