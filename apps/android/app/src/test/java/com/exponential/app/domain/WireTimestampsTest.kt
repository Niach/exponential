package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-169: Room holds BOTH Electric's Postgres text encoding
// (`yyyy-MM-dd HH:mm:ss[.ffffff]+00`) and ISO-8601 from tRPC-era writes; the
// parser must accept both (strict Instant.parse alone blanked every synced
// row's relative time and kept the EXP-153 staleness guard fail-open).
class WireTimestampsTest {

    // 2026-07-01T10:00:00Z
    private val baseMs = 1_782_900_000_000L

    @Test
    fun parsesElectricPostgresText() {
        assertEquals(baseMs, WireTimestamps.parseEpochMs("2026-07-01 10:00:00+00"))
    }

    @Test
    fun parsesElectricPostgresTextWithMicros() {
        assertEquals(baseMs + 123L, WireTimestamps.parseEpochMs("2026-07-01 10:00:00.123456+00"))
    }

    @Test
    fun parsesPostgresTextWithShortFraction() {
        assertEquals(baseMs + 500L, WireTimestamps.parseEpochMs("2026-07-01 10:00:00.5+00"))
    }

    @Test
    fun parsesPostgresTextWithMinuteOffset() {
        // 10:00 at +05:30 is 04:30 UTC.
        assertEquals(baseMs - 19_800_000L, WireTimestamps.parseEpochMs("2026-07-01 10:00:00+05:30"))
    }

    @Test
    fun offsetLessPostgresTextIsUtc() {
        assertEquals(baseMs, WireTimestamps.parseEpochMs("2026-07-01 10:00:00"))
    }

    @Test
    fun parsesIsoInstant() {
        assertEquals(baseMs, WireTimestamps.parseEpochMs("2026-07-01T10:00:00Z"))
        assertEquals(baseMs + 123L, WireTimestamps.parseEpochMs("2026-07-01T10:00:00.123Z"))
    }

    @Test
    fun parsesIsoWithExplicitOffset() {
        // 10:00 at +02:00 is 08:00 UTC.
        assertEquals(baseMs - 7_200_000L, WireTimestamps.parseEpochMs("2026-07-01T10:00:00+02:00"))
    }

    @Test
    fun crossFormatEquality() {
        // The same instant in both wire encodings parses identically.
        assertEquals(
            WireTimestamps.parseEpochMs("2026-07-01 10:00:00.123+00"),
            WireTimestamps.parseEpochMs("2026-07-01T10:00:00.123Z"),
        )
    }

    @Test
    fun trimsWhitespace() {
        assertNotNull(WireTimestamps.parseEpochMs(" 2026-07-01 10:00:00+00 "))
    }

    @Test
    fun rejectsGarbage() {
        assertNull(WireTimestamps.parseEpochMs(""))
        assertNull(WireTimestamps.parseEpochMs("   "))
        assertNull(WireTimestamps.parseEpochMs("not a timestamp"))
        assertNull(WireTimestamps.parseEpochMs("2026-07-01"))
    }
}
