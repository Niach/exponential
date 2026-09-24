package com.exponential.app.ui.components.picker

import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Ignore
import org.junit.Test

/**
 * EXP-1029 contract — the shared picker API on Android. The live cases pin
 * what the stubs already do; the ignored ones are the presentation rules
 * and the typed-picker gate EXP-1021 implements and un-ignores (web, IDE
 * and iOS carry the same case names).
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
        assertEquals(DomainContract.deviceIconValues.size, iconPickerItems(IconPickerSet.Device).size)
    }

    @Ignore("EXP-1029 contract: EXP-1021 moves the account and icon pickers onto Picker")
    @Test
    fun everyTypedPickerRendersThroughThePrimitive() {
        fail("EXP-1021")
    }

    @Ignore("EXP-1029 contract: EXP-1021 implements the picker sheet")
    @Test
    fun aPhoneSheetOfPlainRowsNoCardsInsideTheSheet() {
        fail("EXP-1021")
    }

    @Ignore("EXP-1029 contract: EXP-1021 implements the picker sheet")
    @Test
    fun multiModeMarksPickedRowsByTheHighlightColourNeverACircle() {
        fail("EXP-1021")
    }

    @Ignore("EXP-1029 contract: EXP-1021 implements the picker sheet")
    @Test
    fun singleModeClosesOnAPickMultiModeStaysOpen() {
        fail("EXP-1021")
    }

    @Ignore("EXP-1029 contract: EXP-1021 implements the picker sheet")
    @Test
    fun searchFiltersRowsByLabelAndKeywords() {
        fail("EXP-1021")
    }
}
