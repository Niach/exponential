package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.press
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Round 9 on the Compose painter (docs/round-9-contract.md): check severities
 * (§6.6, `interactions-round1.json` `checkSeverities`, `buttonChecks`), the
 * field's one message in its colour, `accessibility.live` + `hidden` (§5.1)
 * and the Markdown inline profile (§9.1, `markdown-inline.json`).
 * `EXPONENTIAL_UI_ROUND9_PNG=<dir>` saves the two specimens as painted.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class Round9Test {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    private fun fixedModel(json: String, data: JsonValue?, host: HostPlugin): SurfaceModel {
        val m = SurfaceModel("t", SurfaceOptions(), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested(json)
        data?.obj?.forEach { (k, v) -> m.setData("/$k", v) }
        m.setViewport(390f, 800f)
        return m
    }

    @Test
    fun checkSeveritiesShowOneMessageAndOnlyErrorsBlock() {
        val cases = Fixtures.json("interactions-round1.json")["checkSeverities"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]!!.string!!
            val host = RecordingHost()
            val m = fixedModel(
                """{"id":"f","component":"Form","props":{"name":"f"},"on":{"submit":{"event":{"name":"submit"}},"invalid":{"event":{"name":"invalid"}}},"children":[
                {"id":"x","component":"Input","props":{"label":"X","name":"x","value":"v","validateOn":"submit","checks":${c["checks"]!!.json}}},
                {"id":"go","component":"Button","props":{"label":"Send","submit":true}}]}""",
                c["data"],
                host,
            )
            m.press(m.idx("go"))
            assertEquals(name, c["submitted"]!!.bool, host.actions.any { it.name == "submit" })
            val invalid = host.actions.firstOrNull { it.name == "invalid" }
            val errors = (invalid?.payload ?: invalid?.context)?.get("errors")?.array.orEmpty().map { it["message"]?.string }
            assertEquals(name, c["errors"]!!.array!!.map { it.string }, errors)
            val part = m.indexOf("x.error")?.let(m::node)
            val shown = c["shown"]!!
            if (shown.isNull) {
                assertNull(name, part)
            } else {
                assertNotNull(name, part)
                assertEquals(name, shown["message"]!!.string, part!!.props["text"]?.string)
                val severity = shown["severity"]!!.string!!
                assertEquals(name, listOf(if (severity == "error") "invalid" else severity), part.partStates)
            }
            assertEquals(name, c["invalid"]!!.bool, "invalid" in m.node(m.idx("x.field"))!!.partStates)
        }
    }

    @Test
    fun aButtonIsDisabledByAFailingErrorOnly() {
        val cases = Fixtures.json("interactions-round1.json")["buttonChecks"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]!!.string!!
            val host = RecordingHost()
            val m = fixedModel(
                """{"id":"root","component":"Box","children":[{"id":"b","component":"Button","props":{"label":"Go","checks":${c["checks"]!!.json}},"on":{"press":{"event":{"name":"press"}}}}]}""",
                c["data"],
                host,
            )
            val disabled = c["disabled"]!!.bool!!
            assertEquals(name, disabled, m.isDisabled(m.idx("b")))
            m.press(m.idx("b"))
            assertEquals(name, !disabled, host.actions.any { it.name == "press" })
        }
    }

    private fun input(id: String, label: String, value: String, path: String) =
        """{"id":"$id","component":"Input","props":{"label":"$label","name":"$id","value":"$value","validateOn":"submit","checks":[{"condition":{"@path":"$path"}}]}}"""

    /** The styleguide's field states (web `ExponentialUiFieldStates`), revealed by the Form's submit. */
    private val fieldStates = """{"id":"f","component":"Form","props":{"name":"f"},"children":[{"id":"stack","component":"Stack","props":{"gap":"md"},"children":[
        ${input("email", "Email", "ada@", "/error")},${input("password", "Password", "hunter2", "/warning")},${input("nickname", "Nickname", "ada", "/info")},
        {"id":"news","component":"Checkbox","props":{"label":"Newsletter (info)","name":"news","checked":true,"checks":[{"condition":{"@path":"/news"}}]}},
        {"id":"go","component":"Button","props":{"label":"Save","submit":true}}]}]}"""

    private fun result(severity: String, message: String) =
        JsonValue.Obj(mapOf("valid" to JsonValue.Bool(false), "severity" to JsonValue.Str(severity), "message" to JsonValue.Str(message)))

    private fun fieldStatesModel(theme: String = "exponential"): SurfaceModel {
        val m = makeModel(theme = theme)
        m.setNested(fieldStates)
        m.setData("/error", result("error", "Not an email"))
        m.setData("/warning", result("warning", "Weak: add a few more characters"))
        m.setData("/info", result("info", "Shown on your profile"))
        m.setData("/news", result("info", "Monthly at most"))
        return m
    }

    private fun render(m: SurfaceModel, paint: @Composable () -> Unit = { ExponentialSurface(m) }) {
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { paint() } }
        compose.waitForIdle()
    }

    private fun save(name: String) {
        System.getenv("EXPONENTIAL_UI_ROUND9_PNG")?.takeIf { it.isNotEmpty() }?.let { dir ->
            compose.onNodeWithTag("shot").captureRoboImage("$dir/$name", RoborazziOptions(taskType = RoborazziTaskType.Record))
        }
    }

    private fun live(text: String, mode: LiveRegionMode) =
        compose.onNode(hasContentDescription(text).and(SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, mode)), useUnmergedTree = true).assertExists()

    @Test
    fun severitiesPaintTheirColourAndSpeak() {
        for (theme in listOf("neutral", "exponential")) {
            val m = fieldStatesModel(theme)
            m.press(m.idx("go"))
            assertEquals(theme, m.color("warning"), m.inks[m.idx("password.error")])
            assertEquals(theme, m.color("mutedForeground"), m.inks[m.idx("nickname.error")])
            assertEquals(theme, m.color("destructive"), m.inks[m.idx("email.error")])
            // The same `sm` line, and an advisory leaves the control's border alone.
            val text = listOf("email", "password", "nickname").map { m.textStyle(m.idx("$it.error")) }
            assertEquals(theme, 1, text.toSet().size)
            assertEquals(theme, "invalid" in m.node(m.idx("email.field"))!!.partStates, true)
            assertEquals(theme, m.style(m.idx("password.field")).borderColor, m.style(m.idx("nickname.field")).borderColor)
            assertNotEquals(theme, m.style(m.idx("password.field")).borderColor, m.style(m.idx("email.field")).borderColor)
        }
        // Pictured under neutral, what the web and desktop shots use.
        val m = fieldStatesModel("neutral")
        m.press(m.idx("go"))
        // A Checkbox's message sits under its label, the box on the first line.
        val box = m.frame(m.idx("news.box"))
        val message = m.frame(m.idx("news.error"))
        assertTrue("$box $message", message.left >= box.right && message.top >= box.bottom - 4f)
        render(m)
        live("Not an email", LiveRegionMode.Assertive)
        live("Weak: add a few more characters", LiveRegionMode.Polite)
        live("Shown on your profile", LiveRegionMode.Polite)
        // Only the failing error is the field's error.
        val errors = compose.onAllNodes(SemanticsMatcher.keyIsDefined(SemanticsProperties.Error), useUnmergedTree = true).fetchSemanticsNodes()
        assertEquals(listOf("Not an email"), errors.map { it.config[SemanticsProperties.Error] })
        save("android-severity.png")
    }

    /** What apps/android's RecordFormFieldStatesSpecimen does: `validateOn: blur` reveals an advisory. */
    @Test
    fun aBlurRevealsAnAdvisory() {
        val m = makeModel()
        m.setNested("""{"id":"x","component":"Input","props":{"label":"X","name":"x","value":"v","validateOn":"blur","checks":[{"condition":{"@path":"/w"}}]}}""")
        m.setData("/w", result("warning", "Looks short"))
        assertNull(m.indexOf("x.error"))
        m.fieldFocused(m.idx("x.field"), false)
        assertEquals(listOf("warning"), m.node(m.idx("x.error"))!!.partStates)
    }

    @Test
    fun accessibilityLiveWinsAndHiddenLeavesTheTree() {
        val m = makeModel()
        m.setNested(
            """{"id":"root","component":"Box","children":[
            {"id":"a","component":"Text","props":{"text":"Alpha","live":"polite"},"accessibility":{"live":"assertive"}},
            {"id":"b","component":"Text","props":{"text":"Beta","live":"polite"},"accessibility":{"live":"off"}},
            {"id":"c","component":"Text","props":{"text":"Gamma","live":"assertive"}},
            {"id":"d","component":"Box","accessibility":{"hidden":{"@path":"/hide"}},"children":[{"id":"e","component":"Text","props":{"text":"Delta"}}]},
            {"id":"f","component":"Text","props":{"text":"Epsilon"},"accessibility":{"hidden":true}}]}""",
        )
        m.setData("/hide", JsonValue.Bool(true))
        render(m)
        live("Alpha", LiveRegionMode.Assertive)
        live("Gamma", LiveRegionMode.Assertive)
        val beta = compose.onNode(hasContentDescription("Beta"), useUnmergedTree = true).fetchSemanticsNode()
        assertFalse(SemanticsProperties.LiveRegion in beta.config)
        // The merged tree: what assistive tech reads.
        compose.onNode(hasContentDescription("Gamma")).assertExists()
        compose.onNode(hasContentDescription("Delta")).assertDoesNotExist()
        compose.onNode(hasContentDescription("Epsilon")).assertDoesNotExist()
    }

    /** `$type.size.sm` in [theme]'s resolved tokens. */
    private fun token(theme: ThemeHandle, ref: String): JsonValue =
        ref.removePrefix("$").split(".").fold(theme.tokens) { at, key -> at[key] ?: error("no token $ref") }

    @Test
    fun inlineRecipesReplayTheFixture() {
        val neutral = ThemeHandle.builtin("neutral")!!
        val base = ResolvedTextStyle(14f, 400, 20f, null)
        fun styles(chat: Boolean, variant: String): at.exponential.ui.primitives.MarkdownStyles {
            val props: Props = mapOf("profile" to JsonValue.Str("inline"), "variant" to JsonValue.Str(variant))
            return MarkdownPainter.styles(neutral, Mode.Dark, base, MarkdownPainter.props(props, chat))
        }
        val cases = Fixtures.json("markdown-inline.json")["cases"]!!.array!!
        assertEquals(8, cases.size)
        for (c in cases) {
            val part = c["part"]!!.string!!
            val variant = c["variant"]?.string ?: "body"
            val level = c["level"]?.number?.toInt() ?: 2
            val name = "$part ${c["variant"]?.string ?: level}"
            val recipes = MarkdownPainter.inlineRecipes(part, variant, level)
            val want = c["recipes"]!!.array!!.map { MarkdownPainter.InlineRecipe(it["component"]!!.string!!, it["part"]!!.string!!, it["props"]!!.obj!!) }
            assertEquals(name, want, recipes)
            val n = c["neutral"]!!
            // The chat rhythm reaches neither part.
            for (chat in listOf(false, true)) {
                val s = styles(chat, variant)
                val spec = if (part == "paragraph") s.paragraph!! else s.headingLevels[level]!!
                assertEquals(name, token(neutral, n["fontSize"]!!.string!!).number!!.toFloat(), spec.size)
                assertEquals(name, token(neutral, n["lineHeight"]!!.string!!).number!!.toFloat(), spec.lineHeight)
                assertEquals(name, token(neutral, n["fontWeight"]!!.string!!).number!!.toInt(), spec.weight)
                assertEquals(name, neutral.sansFamily, spec.family)
                assertEquals(name, neutral.spacing("sm"), s.blockGap)
                assertEquals(name, neutral.spacing("md"), s.headingTops[level])
                assertEquals(name, neutral.spacing("xs"), s.headingBottoms[level])
            }
            assertEquals(name, neutral.color(n["color"]!!.string!!.removePrefix("\$color."), Mode.Dark), MarkdownPainter.inlineInk(neutral, Mode.Dark, recipes))
        }
    }

    private fun markdown(text: String, profile: String = "inline", variant: String = "body") =
        """{"id":"root","component":"Box","children":[{"id":"m","component":"Markdown","props":{"text":${JsonValue.Str(text).json},"profile":"$profile","variant":"$variant"}}]}"""

    @Test
    fun theDocumentPaintsTheFixturesCodeAndLists() {
        val neutral = ThemeHandle.builtin("neutral")!!
        val paint = Fixtures.json("markdown-inline.json")["documentPaint"]!!
        val want = paint["code"]!!["neutral"]!!
        val s = MarkdownPainter.styles(neutral, Mode.Dark, ResolvedTextStyle(14f, 400, 20f, null), emptyMap())
        val code = s.chip.code
        assertEquals(token(neutral, want["fontSize"]!!.string!!).number!!.toFloat(), code.size)
        assertEquals(token(neutral, want["borderRadius"]!!.string!!).number!!.toFloat(), code.radius)
        assertEquals(token(neutral, want["paddingHorizontal"]!!.string!!).number!!.toFloat(), code.padH)
        assertEquals(0f, code.padV)
        val list = paint["list"]!!
        assertEquals(kotlin.math.round(14f * list["indentEm"]!!.number!!.toFloat()), s.listIndent)
        assertEquals(list["bulletEm"]!!.number!!.toFloat(), at.exponential.ui.primitives.MARKDOWN_BULLET_EM)
        assertEquals(list["bulletCentreEm"]!!.number!!.toFloat(), at.exponential.ui.primitives.MARKDOWN_BULLET_CENTRE_EM)
        assertEquals(list["itemGap"]!!.number!!.toFloat(), s.itemTop)
        assertEquals(list["itemGap"]!!.number!!.toFloat(), s.itemBottom)
    }

    @Test
    fun inlineCodeReservesItsPaddingOnBothSides() {
        val style = at.exponential.ui.primitives.MarkdownChipStyle(code = at.exponential.ui.primitives.MarkdownCodeStyle(size = 12f, padH = 4f, radius = 6f))
        val spec = at.exponential.ui.primitives.MarkdownTextSpec(14f, 20f, 400, null)
        val inlines = listOf(at.exponential.ui.primitives.MarkdownInline(text = "a "), at.exponential.ui.primitives.MarkdownInline(text = "code", code = true))
        val pads = at.exponential.ui.primitives.markdownChipPlaceholders(inlines, spec, style, null) { _, _ -> 0f }
        assertEquals(listOf(2 to 3, 7 to 8), pads.map { it.start to it.end })
        assertEquals(listOf(4f, 4f), pads.map { it.item.width.value })
    }

    @Test
    fun anInlineMarkdownMeasuresItsVariantAndNoGapAroundIt() {
        val theme = ThemeHandle.builtin("neutral")!!
        val caption = theme.part("Text", "root", mapOf("variant" to JsonValue.Str("caption")), mode = Mode.Dark)
        val m = makeModel(theme = "neutral")
        m.setNested(markdown("One line", variant = "caption"))
        assertEquals(caption.px("lineHeight")!!, m.frame(m.idx("m")).height, 0.01f)
        // A heading's top margin drops as the first block; its bottom gap stays between blocks.
        val h1 = theme.part("Heading", "root", mapOf("level" to JsonValue.Str("h1")), mode = Mode.Dark)
        m.setNested(markdown("# Title\n\nOne line"))
        val body = theme.part("Text", "root", mapOf("variant" to JsonValue.Str("body")), mode = Mode.Dark)
        assertEquals(h1.px("lineHeight")!! + theme.spacing("xs") + body.px("lineHeight")!!, m.frame(m.idx("m")).height, 0.01f)
    }

    @Test
    fun anInlineMarkdownSkipsTheHostsPainter() {
        val asked = ArrayList<String>()
        val host = object : HostPlugin {
            override fun markdown(text: String, width: Float): (@Composable () -> Unit)? {
                asked.add(text)
                return { BasicText("host") }
            }
        }
        val m = makeModel(host = host).also { it.setNested(markdown("Inline")) }
        val document = makeModel(host = host).also { it.setNested(markdown("Document", profile = "document")) }
        render(m) {
            Column {
                ExponentialSurface(m)
                ExponentialSurface(document)
            }
        }
        assertEquals(setOf("Document"), asked.toSet())
    }

    @Test
    fun theMarkdownInlineSpecimen() {
        val m = makeModel(theme = "neutral")
        m.setNested(
            """{"id":"root","component":"Stack","props":{"gap":"lg"},"children":[
            {"id":"t","component":"Text","props":{"text":"A plain Text beside its inline twin"}},
            {"id":"body","component":"Markdown","props":{"text":"A plain Text beside its inline twin","profile":"inline"}},
            {"id":"caption","component":"Markdown","props":{"text":"Caption **strong**, with a [link](https://exponential.at) and `code`","profile":"inline","variant":"caption"}},
            {"id":"headings","component":"Markdown","props":{"text":"# Heading 1\n\n## Heading 2\n\n### Heading 3\n\n#### Heading 4\n\n##### Heading 5 paints h4\n\n###### Heading 6 paints h4\n\nA paragraph under them.\n\n- a list stays\n- the document's","profile":"inline"}},
            {"id":"doc","component":"Markdown","props":{"text":"## A title\n\nA **bold** paragraph with `code` and [a link](https://exponential.at).\n\n- one\n- two\n\nLast paragraph."}}]}""",
        )
        assertEquals(m.frame(m.idx("t")).height, m.frame(m.idx("body")).height, 0.01f)
        render(m)
        save("android-markdown-inline.png")
    }
}
