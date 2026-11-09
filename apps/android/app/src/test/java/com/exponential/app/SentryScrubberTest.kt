package com.exponential.app

import io.sentry.Breadcrumb
import io.sentry.SentryEvent
import io.sentry.protocol.Request
import io.sentry.protocol.Response
import io.sentry.protocol.User
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #LOGS-2: nothing credential-like leaves the phone. */
class SentryScrubberTest {
    private val f = SentryScrubber.FILTERED

    @Test
    fun `sensitive keys match the shared pattern`() {
        for (key in listOf("Authorization", "Cookie", "Set-Cookie", "token", "x-api-key", "client_secret", "password", "steer_ticket", "sessionId", "SENTRY_DSN")) {
            assertTrue(key, SentryScrubber.isSensitiveKey(key))
        }
        for (key in listOf("url", "method", "status_code", "x-client-version", "table", "offset")) {
            assertFalse(key, SentryScrubber.isSensitiveKey(key))
        }
    }

    @Test
    fun `query values of sensitive params are filtered, others kept`() {
        assertEquals(
            "https://app.exponential.at/api/shapes/issues?table=issues&token=$f&live=true&handle=h1",
            SentryScrubber.scrubUrl("https://app.exponential.at/api/shapes/issues?table=issues&token=abc&live=true&handle=h1"),
        )
        assertEquals("https://x/y?ticket=$f#frag", SentryScrubber.scrubUrl("https://x/y?ticket=t1#frag"))
        assertEquals("https://x/y", SentryScrubber.scrubUrl("https://x/y"))
    }

    @Test
    fun `event request, response, extras, tags, breadcrumbs and user are scrubbed`() {
        val event = SentryEvent().apply {
            request = Request().apply {
                url = "https://app.exponential.at/api/trpc/x?session_token=s"
                queryString = "a=1&password=p"
                headers = mapOf("Authorization" to "Bearer t", "Cookie" to "c=1", "Accept" to "application/json")
                cookies = "c=1"
            }
            contexts.setResponse(Response().apply {
                headers = mapOf("Set-Cookie" to "c=1", "Content-Type" to "text/plain")
                cookies = "c=1"
            })
            setExtra("refreshToken", "r")
            setExtra("count", 3)
            setTag("dsn", "https://k@host/1")
            setTag("screen", "home")
            addBreadcrumb(Breadcrumb().apply { setData("url", "https://x/?key=k"); setData("method", "GET") })
            user = User().apply { id = "u1"; email = "a@b.c"; username = "a"; ipAddress = "1.2.3.4" }
        }

        val out = SentryScrubber.scrub(event)

        val req = out.request!!
        assertEquals("https://app.exponential.at/api/trpc/x?session_token=$f", req.url)
        assertEquals("a=1&password=$f", req.queryString)
        assertEquals(mapOf("Authorization" to f, "Cookie" to f, "Accept" to "application/json"), req.headers)
        assertNull(req.cookies)
        val res = out.contexts.response!!
        assertEquals(mapOf("Set-Cookie" to f, "Content-Type" to "text/plain"), res.headers)
        assertNull(res.cookies)
        assertEquals(f, out.getExtra("refreshToken"))
        assertEquals(3, out.getExtra("count"))
        assertEquals(f, out.getTag("dsn"))
        assertEquals("home", out.getTag("screen"))
        assertEquals("https://x/?key=$f", out.breadcrumbs!!.single().getData("url"))
        assertEquals("GET", out.breadcrumbs!!.single().getData("method"))
        assertEquals("u1", out.user!!.id)
        assertNull(out.user!!.email)
        assertNull(out.user!!.username)
        assertNull(out.user!!.ipAddress)
    }

    @Test
    fun `breadcrumb data is scrubbed`() {
        val crumb = Breadcrumb().apply {
            setData("auth_token", "t")
            setData("url", "https://x/api/shapes/a?secret=s&offset=-1")
            setData("status_code", 200)
        }
        SentryScrubber.scrub(crumb)
        assertEquals(f, crumb.getData("auth_token"))
        assertEquals("https://x/api/shapes/a?secret=$f&offset=-1", crumb.getData("url"))
        assertEquals(200, crumb.getData("status_code"))
    }

    @Test
    fun `only API and shape requests get spans`() {
        assertTrue(Observability.isTracedPath("/api/trpc/issues.list"))
        assertTrue(Observability.isTracedPath("/api/shapes/issues"))
        assertFalse(Observability.isTracedPath("/api/attachments/abc"))
        assertFalse(Observability.isTracedPath("/api/monitoring"))
        assertFalse(Observability.isTracedPath("/avatars/x.png"))
    }
}
