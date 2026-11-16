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

    /** #ERR-28: the allocation-free path reads every shape the java.time path does, to the millisecond. */
    @Test
    fun fastPathMatchesJavaTime() {
        val random = java.util.Random(28)
        repeat(20_000) {
            val ms = random.nextLong() % 4_102_444_800_000L // ±2100
            val instant = java.time.Instant.ofEpochMilli(if (ms < 0) -ms else ms)
            val offsetMinutes = (random.nextInt(29) - 14) * 60 + if (random.nextBoolean()) 30 else 0
            val offset = java.time.ZoneOffset.ofTotalSeconds(offsetMinutes * 60)
            val local = instant.atOffset(offset).toLocalDateTime()
            val digits = random.nextInt(10)
            val fraction = if (digits == 0) "" else "." + "%09d".format(local.nano).substring(0, digits)
            val truncatedMs = instant.toEpochMilli() - instant.toEpochMilli() % 1000 +
                (if (digits == 0) 0 else "%09d".format(local.nano).substring(0, digits).padEnd(9, '0').substring(0, 3).toLong())
            val base = "%04d-%02d-%02d".format(local.year, local.monthValue, local.dayOfMonth) +
                "%s%02d:%02d:%02d".format("T", local.hour, local.minute, local.second) + fraction
            val iso = base + if (offsetMinutes == 0) "Z" else offset.id
            val pg = base.replace('T', ' ') + if (offsetMinutes % 60 == 0) "%+03d".format(offsetMinutes / 60) else offset.id
            assertEquals(iso, truncatedMs, WireTimestamps.parseEpochMs(iso))
            assertEquals(pg, truncatedMs, WireTimestamps.parseEpochMs(pg))
        }
        // Not a date: still rejected (the slow path decides).
        assertNull(WireTimestamps.parseEpochMs("2026-02-30 10:00:00+00"))
        assertNull(WireTimestamps.parseEpochMs("2026-07-01T10:00:00"))
        assertNull(WireTimestamps.parseEpochMs("2026-13-01 10:00:00+00"))
    }

    /** #ERR-28 memory budget: a big chat's open parses ~95k stamps; java.time spent ~600 B each. */
    @Test
    fun parsingAllocatesNothingPerStamp() {
        val stamps = Array(1_000) { "2026-07-01 10:%02d:%02d.%06d+00".format(it / 60 % 60, it % 60, it * 997) } +
            Array(1_000) { "2026-07-01T10:%02d:%02d.%03dZ".format(it / 60 % 60, it % 60, it) }
        repeat(3) { stamps.forEach(WireTimestamps::parseEpochMs) } // warm up
        val (bytes, sum) = allocatedBytes {
            var sum = 0L
            repeat(50) { for (s in stamps) sum += WireTimestamps.parseEpochMs(s)!! }
            sum
        }
        org.junit.Assert.assertTrue(sum != 0L)
        // 100k parses: boxing the Long? result at most (16 B each).
        org.junit.Assert.assertTrue("100k parses allocated $bytes bytes", bytes < 100_000L * 24)
    }
}
