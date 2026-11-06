package at.exponential.runner

import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** lifecycle.json `deepLink`: `<runner.json id>://<screen>?<params>`. */
class RunnerLinksTest {
    private fun event(uri: String) = RunnerLinks.event(uri, "at.acme.album")?.let(JsonValue::parse)

    @Test
    fun parsesTheScreenAndItsParams() {
        assertEquals(
            JsonValue.parse("""{"type":"deepLink","screen":"pages","params":{"album":"a 1","empty":"","x":"é"}}"""),
            event("at.acme.album://pages?album=a%201&empty&x=%C3%A9"),
        )
        assertEquals(JsonValue.parse("""{"type":"deepLink","screen":"albums","params":{}}"""), event("AT.ACME.ALBUM://albums"))
    }

    @Test
    fun anotherSchemeIsNotOurs() {
        assertNull(event("https://example.com/pages"))
        assertNull(event("at.acme.other://pages"))
        assertNull(event("not a uri"))
        assertNull(event("at.acme.album:opaque"))
    }
}
