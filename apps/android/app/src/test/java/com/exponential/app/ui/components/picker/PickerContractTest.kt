package com.exponential.app.ui.components.picker

import androidx.compose.ui.graphics.Color
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.ui.components.toPickerDevice
import com.exponential.app.ui.icons.ExpIcons
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
        // EXP-1021: the status glyph rides along, but the LABEL stays the
        // one-line `IDENT Title` the four clients are fixture-locked to.
        val withGlyph = issuePickerItems(
            listOf(IssuePickerIssue("i", "APP-1", "Fix", icon = ExpIcons.statusBacklog)),
        ).single()
        assertEquals("APP-1 Fix", withGlyph.label)
        assertTrue(withGlyph.icon != null)
        assertEquals(
            listOf("Unassigned", "Ada"),
            assigneePickerItems(listOf(AssigneePickerMember("u", "Ada")), allowsNone = true).map { it.label },
        )
        // The icon picker is the tenth, and the SET it offers is the registry's
        // own append-only list (EXP-924) — `laptop` is a device glyph and has
        // never been a board one. It renders those names as a grid rather than
        // as rows, so there is no `iconPickerItems` here to build: the picker
        // package holds no dead row builder for it.
        assertTrue(ExpIcons.devicePickable.contains("laptop"))
        assertFalse(ExpIcons.pickable.contains("laptop"))
        // A machine always resolves a glyph, and a SERVER never wears the
        // desktop default.
        val server = devicePickerItems(
            listOf(DevicePickerDevice(id = "d", name = "buildbox", isServer = true)),
        ).single()
        assertEquals(ExpIcons.uiServer, server.icon)
        assertEquals(
            ExpIcons.uiDevice,
            devicePickerItems(listOf(DevicePickerDevice(id = "d", name = "mbp"))).single().icon,
        )
        // A login row searches on its email AND its agent; the health badge is
        // its note.
        val account = accountPickerItems(
            listOf(
                AccountPickerOption(
                    key = "claude:system",
                    agent = "claude",
                    email = "ada@example.com",
                    healthNote = "Needs re-login",
                ),
            ),
        ).single()
        assertEquals("ada@example.com", account.label)
        assertEquals("Needs re-login", account.description)
        assertEquals(listOf("ada@example.com", "claude"), account.keywords)
    }

    /**
     * EXP-992's rate-limit preview survived the move onto the primitive: the
     * bars ride the option and the picker draws them in the row's own body
     * ([Picker]'s `renderItem`), because they stack UNDER the email and a
     * brand mark is a drawable rather than an `AppIcons` vector.
     */
    @Test
    fun anAccountRowKeepsItsRateLimitPreview() {
        val source = pickerSource("AccountPicker.kt")
        assertTrue("the option carries the windows", source.contains("val limits: AccountLimits?"))
        assertTrue("the row draws them", source.contains("AccountLimitBars("))
        assertTrue("over the primitive's row", source.contains("renderItem = "))
    }

    /**
     * A status row is a GLYPH row on every client: an unresolvable registry
     * name degrades to the neutral backlog glyph rather than blanking the
     * leading slot — a coloured row with no glyph would silently drop to the
     * DOT a label row wears (`IssueVisuals`' `StatusIcon` rule).
     */
    @Test
    fun aStatusRowCanNeverLoseItsGlyph() {
        val rows = statusPickerItems(
            listOf(
                StatusPickerStatus(id = "s", name = "Shipping", category = "started"),
                StatusPickerStatus(
                    id = "t",
                    name = "Unknown",
                    category = "backlog",
                    iconName = "not-a-registry-name",
                    colorHex = "#EF4444",
                ),
            ),
        )
        assertTrue(rows.all { it.icon != null })
        assertEquals(ExpIcons.statusBacklog, rows[1].icon)
    }

    /**
     * The 44dp row is the HIGHLIGHT's height: the minimum has to sit ABOVE the
     * inner padding in the modifier chain (`GlassSheetRow`'s order). Below it
     * the padding is added on top of the minimum and a picker row stands ~18dp
     * taller than the sheet rows beside it — the same list, two rhythms.
     */
    @Test
    fun aRowIsTheSheetRowsHeightNotThatPlusItsPadding() {
        val source = pickerSource("Picker.kt")
        val minHeight = source.indexOf("defaultMinSize(minHeight = PickerDefaults.RowHeight)")
        val innerPadding = source.indexOf("horizontal = PickerDefaults.RowInnerPadding")
        assertTrue(minHeight > 0 && innerPadding > 0)
        assertTrue("the min height goes above the inner padding", minHeight < innerPadding)
        assertEquals(44, PickerDefaults.RowHeight.value.toInt())
    }

    /**
     * The sheet carries ONE row idiom, footer included: an action row is the
     * picker's own ([PickerActionRow]), never a row borrowed from another
     * surface. The icon picker is the case that regressed — its "No icon"
     * reset sat inside the sheet as a `GlassSheetRow`.
     */
    @Test
    fun aFooterActionIsAPickerRowToo() {
        val source = pickerSource("Picker.kt")
        assertTrue(source.contains("fun PickerActionRow("))
        val iconPicker = moduleFile("src/main/java/com/exponential/app/ui/components/IconPicker.kt").readText()
        assertTrue(iconPicker.contains("PickerActionRow("))
        assertFalse("no second row idiom in the sheet", iconPicker.contains("GlassSheetRow("))
        val labelSheet = moduleFile("src/main/java/com/exponential/app/ui/issue/LabelPickerSheet.kt").readText()
        assertTrue(labelSheet.contains("PickerActionRow("))
        assertFalse("no second row idiom in the sheet", labelSheet.contains("GlassSheetRow("))
    }

    /**
     * A footer under a PANEL has to keep its height. `GlassSheet` caps its
     * column (85 % of the screen on a fitted sheet) and measures its
     * UNWEIGHTED children against that whole cap first — an unweighted
     * scrolling panel therefore eats the main axis and the footer beside it
     * measures at maxHeight 0, which is not a short row but NO row. That is
     * exactly how the icon picker's "No icon" reset disappeared on a phone:
     * the board set is 7 columns x 14 rows, over the cap on every handset, so
     * an `allowsNone` host could not clear an icon at all.
     *
     * So the panel is the one child that may grow, `fill = false` so a short
     * one still takes only what it needs, and the footer is its SIBLING (in
     * the rows branch it is a list item instead, where it scrolls with them).
     */
    @Test
    fun aPanelLeavesRoomForTheFooterUnderIt() {
        val source = pickerSource("Picker.kt")
        val branch = source.indexOf("if (panel != null) {")
        val bounded = source.indexOf("Modifier.weight(1f, fill = false)")
        val panelCall = source.indexOf("panel { picked ->")
        val footer = source.indexOf("footer?.invoke()")
        assertTrue(branch > 0 && bounded > 0 && panelCall > 0 && footer > 0)
        assertTrue("the panel slot is bounded inside the panel branch", branch < bounded)
        assertTrue("and the panel renders inside that slot", bounded < panelCall)
        assertTrue("with the footer under it, not inside it", panelCall < footer)
        // The ROW branch's footer scrolls with the rows instead, so it is a
        // list item rather than a sibling of the scroller.
        assertTrue(source.contains("if (footer != null) item { footer() }"))
    }

    /**
     * The ONE recorded exception to "a board list is a [BoardPicker]": the
     * share composer's target sheet (`ui/share/ShareBoardPicker.kt`) spans
     * TEAMS, and the shared `BoardPickerBoard` contract (web
     * `board-picker.tsx`) carries no team. A picker row is FLAT — the contract
     * has no section header — so it rides the PRIMITIVE with
     * [boardPickerItems]' rows and says the team in the row's own muted second
     * line. What it may not do is keep a selection language of its own.
     */
    @Test
    fun theShareTargetSheetIsAFlatPickerWithItsTeamAsTheDescription() {
        val share = moduleFile("src/main/java/com/exponential/app/ui/share/ShareBoardPicker.kt").readText()
        assertTrue("it renders the primitive", share.contains("    Picker("))
        assertTrue("over the shared board rows", share.contains("boardPickerItems("))
        assertTrue("the team is the row's description", share.contains("description = group.team.name"))
        assertFalse("never a sheet of its own", share.contains("GlassSheet("))
        assertFalse("never a second picked idiom", share.contains("GlassSheetRow("))
        // The form row that opens option sheets everywhere else went the same
        // way, so no surface reaching it draws a trailing check either.
        val optionRows = moduleFile("src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt").readText()
        assertTrue("the form row is the picker's trigger", optionRows.contains("    Picker("))
        assertFalse("and never draws its own option sheet", optionRows.contains("GlassSheet(title = label"))
    }

    /**
     * A panel REPLACES the rows, so it is handed none — and it does not get to
     * invent its own pick either: it reports the value it was tapped on and
     * the primitive folds it into the selection.
     */
    @Test
    fun aPanelPicksThroughThePrimitive() {
        assertTrue(pickerSource("Picker.kt").contains("panel: (@Composable (pick: (T) -> Unit) -> Unit)?"))
        val iconPicker = moduleFile("src/main/java/com/exponential/app/ui/components/IconPicker.kt").readText()
        assertTrue(iconPicker.contains("panel = { pick ->"))
        assertTrue("no rows are built for a list nothing renders", iconPicker.contains("items = emptyList()"))
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
     * The other half of "one picker per thing": a typed picker with NO caller
     * is not a sweep, it is a second implementation waiting to be skipped. The
     * app must reach every one of the ten from a real surface.
     */
    @Test
    fun everyTypedPickerHasACaller() {
        val callers = moduleDir("src/main/java")
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            // The picker package declares them; so does the icon picker's own
            // file. A declaration is not a caller.
            .filterNot { it.parentFile?.name == "picker" || it.name == "IconPicker.kt" }
            .map { it.readText() }
            .toList()
        val typed = listOf(
            "BoardPicker", "IssuePicker", "ActionPicker", "AccountPicker", "DevicePicker",
            "AssigneePicker", "StatusPicker", "PriorityPicker", "LabelPicker", "IconPicker",
        )
        for (name in typed) {
            assertTrue("$name is rendered by nothing", callers.any { it.contains("$name(") })
        }
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
        // A row at rest paints nothing at all — no fill, no stroke.
        assertEquals(Color.Transparent, PickerDefaults.rowBackground(PickerChecked.None))
        assertEquals(Color.Transparent, PickerDefaults.rowStroke(PickerChecked.None))
    }

    /**
     * The selection language EXP-1021 asked for: a picked row reads as the
     * row's own highlight, never a leading circle or a trailing check.
     */
    @Test
    fun multiModeMarksPickedRowsByTheHighlightColourNeverACircle() {
        assertEquals(GlassTokens.RowFillActive, PickerDefaults.rowBackground(PickerChecked.All))
        assertEquals(GlassTokens.StrokeActive, PickerDefaults.rowStroke(PickerChecked.All))
        assertEquals(Color.Transparent, PickerDefaults.rowBackground(PickerChecked.None))
        val source = pickerSource("Picker.kt")
        for (circle in listOf("uiCheck", "Checkbox", "RadioButton", "uiMinus")) {
            assertFalse("a picked row is a highlight, never a $circle", source.contains(circle))
        }
        // Tri-state (the bulk edit): a PARTIAL row wears the wash alone, so
        // "some of these" is said in paint and never in a dash glyph. An
        // explicit `checked` wins over membership in the set; without one the
        // state is derived from it.
        assertEquals(GlassTokens.RowFillActive, PickerDefaults.rowBackground(PickerChecked.Some))
        assertEquals(Color.Transparent, PickerDefaults.rowStroke(PickerChecked.Some))
        val partial = PickerItem(value = "a", label = "Alpha", checked = PickerChecked.Some)
        assertEquals(PickerChecked.Some, PickerRules.checked(partial, setOf("a")))
        assertEquals(PickerChecked.Some, PickerRules.checked(partial, emptySet()))
        assertEquals(PickerChecked.All, PickerRules.checked(items()[0], setOf("a")))
        assertEquals(PickerChecked.None, PickerRules.checked(items()[0], emptySet()))
        // A partial row still toggles ON: the bulk edit adds it to the rows
        // that are missing it, which is what its absence from `value` means.
        assertEquals(setOf("a"), PickerRules.select(PickerMode.Multi, emptySet(), partial))
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
        assertNull("an ordinary label row derives its state from the set", label.checked)
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

    /**
     * A machine that cannot take a start renders DISABLED with the reason as
     * its description — a documented device-picker rule that nothing could
     * reach while the adapter dropped both fields on the floor.
     * [com.exponential.app.domain.LaunchDeviceRules] owns the verdict and the
     * sentence, so a sheet row never invents a third wording.
     */
    @Test
    fun anUnstartableMachineIsADisabledRowThatSaysWhy() {
        val offline = SteerDevice(deviceId = "d", deviceLabel = "buildbox", online = false)
            .toPickerDevice()
        assertTrue(offline.disabled)
        assertEquals("Offline", offline.description)
        val signedOut = SteerDevice(
            deviceId = "e",
            deviceLabel = "mbp",
            // Online, installed, and every login dead — as unstartable as an
            // offline machine, and the one case the row can name the agent of.
            agents = emptyList(),
            unauthedAgents = listOf("claude"),
        ).toPickerDevice()
        assertTrue(signedOut.disabled)
        assertEquals("claude not signed in", signedOut.description)
        val ready = SteerDevice(deviceId = "f", deviceLabel = "mbp", agents = listOf("claude"))
            .toPickerDevice()
        assertFalse(ready.disabled)
        assertNull(ready.description)
        // And the primitive already refuses to pick one.
        assertNull(PickerRules.select(PickerMode.Single, emptySet(), devicePickerItems(listOf(offline)).single()))
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

    /** [moduleFile] for a source ROOT — the sweep case walks one. */
    private fun moduleDir(path: String): File =
        listOf(path, "app/$path", "apps/android/app/$path")
            .map(::File)
            .firstOrNull { it.isDirectory }
            ?: error("$path not found from ${File(".").absolutePath}")
}
