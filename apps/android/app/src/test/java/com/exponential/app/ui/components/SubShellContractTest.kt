package com.exponential.app.ui.components

import org.junit.Assert.fail
import org.junit.Ignore
import org.junit.Test

/**
 * EXP-1029 contract — sub-shell navigation on Android, ignored until
 * EXP-1020 (web, IDE and iOS carry the same case names).
 */
class SubShellContractTest {

    @Ignore("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    @Test
    fun tappingTheRowSlidesTheChildPageInPlaceOfTheWholeCard() {
        fail("EXP-1020")
    }

    @Ignore("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    @Test
    fun theChildPageCarriesABackButtonOnTopThatReturnsToTheCard() {
        fail("EXP-1020")
    }

    @Ignore("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    @Test
    fun aSubShellInsideTheChildPageSlidesOneLevelDeeper() {
        fail("EXP-1020")
    }

    @Ignore("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    @Test
    fun aDisabledRowNeverOpens() {
        fail("EXP-1020")
    }
}
