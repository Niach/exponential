package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.runPackageTestsJson
import at.exponential.ui.ffi.validatePackageJson
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-196: `fixtures/package-tests.json` (`runs`, `validate`) over the FFI:
 * `runs` through `runPackageTestsJson`, `validate` through
 * `validatePackageJson` (a failing test = a `/tests/<i>` install issue). The
 * TS reference, the Rust core, the FFI facade and Swift's
 * `PackageTestsTests` replay the same cases.
 */
class PackageTestsTest {
    private fun same(got: JsonValue, want: JsonValue, name: String) =
        assertTrue("$name: differs at ${jsonDiff(got.json, want.json)}: got ${got.json.take(300)}", jsonEqual(got.json, want.json))

    private fun fixture(): JsonValue {
        Fixtures.require()
        return Fixtures.json("package-tests.json")
    }

    @Test
    fun runs() {
        val f = fixture()
        val pkg = LinkedHashMap(f["package"]!!.obj.orEmpty())
        pkg["tests"] = f["runs"]!!["tests"]!!
        same(JsonValue.parse(runPackageTestsJson(JsonValue.Obj(pkg).json)), f["runs"]!!["results"]!!, "runs")
        assertEquals("[]", runPackageTestsJson(f["package"]!!.json))
    }

    @Test
    fun validate() {
        val f = fixture()
        val base = f["package"]!!
        val cases = f["validate"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val pkg = LinkedHashMap(base.obj.orEmpty())
            for ((k, v) in c["with"]?.obj.orEmpty()) pkg[k] = v
            same(JsonValue.parse(validatePackageJson(JsonValue.Obj(pkg).json, null)), c["issues"] ?: JsonValue.Arr(emptyList()), "validate: ${c["name"]?.string}")
        }
    }
}
