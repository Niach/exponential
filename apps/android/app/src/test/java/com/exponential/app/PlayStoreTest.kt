package com.exponential.app

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-260: the "Rate our app" row and the 426 update gate both link to the
 * PUBLISHED listing. The staging flavor installs as `at.exponential.staging`,
 * which has no listing of its own, so the suffix must be stripped — otherwise
 * both surfaces open a Play 404.
 */
class PlayStoreTest {
    @Test
    fun stripsStagingSuffixFromAppId() {
        assertEquals("at.exponential", PlayStore.listingAppId("at.exponential.staging"))
        assertEquals("at.exponential", PlayStore.listingAppId("at.exponential"))
    }

    @Test
    fun buildsPlayLinks() {
        assertEquals("market://details?id=at.exponential", PlayStore.marketUri("at.exponential"))
        assertEquals(
            "https://play.google.com/store/apps/details?id=at.exponential",
            PlayStore.webUrl("at.exponential"),
        )
    }
}
