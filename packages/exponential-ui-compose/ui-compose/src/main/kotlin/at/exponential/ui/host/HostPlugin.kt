package at.exponential.ui.host

import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.ExponentialUi
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.filesPicked

/** A user action: a node's `on.<event>` with an `event` action, resolved. */
data class SurfaceActionEvent(
    val surfaceId: String,
    /** The catalog event (`press`, `change`, `select`, `submit`…). */
    val event: String,
    /** The action's `event.name`. */
    val name: String,
    val componentId: String,
    val context: JsonValue,
    /** What the component adds (a Select's `value`, a Tabs' `value`…). */
    val payload: JsonValue?,
)

/**
 * A function call the core does not run itself: an `on.<event>` with
 * `{functionCall: {call, args}}` (A2UI) to a
 * name outside the catalog's built-ins. An [ExponentialHost] gates it
 * through its policy and runs the registered handler.
 */
data class SurfaceFunctionCallEvent(
    val surfaceId: String,
    val componentId: String,
    /** The function name (`call`). */
    val name: String,
    /** The resolved `args` (an object; empty when the call has none). */
    val args: Map<String, JsonValue>,
)

/** What the media loader fetches: the absolute url and the headers to send (auth for `/api/attachments`). */
data class MediaRequest(val url: String, val headers: Map<String, String> = emptyMap())

/** `change` (debounced while typing) or `commit` (blur / Enter). */
enum class InputKind { Change, Commit }

/**
 * A host-owned input edit. `change` fires debounced (150 ms) while typing
 * with a monotonically increasing `revision`; `commit` on blur / Enter.
 */
data class SurfaceInputEvent(
    val surfaceId: String,
    val componentId: String,
    /** The input's `name` prop. */
    val name: String,
    /** The bound data model pointer, when `value` is a binding. */
    val path: String?,
    val value: JsonValue,
    val revision: Int,
    val kind: InputKind,
)

/** A file the host picked for a FileUpload (the bytes stay with the host). */
data class PickedFile(val name: String, val size: Long, val type: String)

/**
 * #EDE-65 R (interactions-round1.json composerPaste): images pasted into
 * Composer [componentId] ([name] = its id): [files] = their name, size and
 * type (the `attach` payload), [uris] = where their bytes are (content URIs
 * the app's ContentResolver opens), in the same order.
 */
data class UploadEvent(val surfaceId: String, val componentId: String, val name: String, val files: List<PickedFile>, val uris: List<String>)

/**
 * A FileUpload asked for files (its drop zone or Browse was pressed):
 * `accept` = the MIME / extension list, `multiple` = more than one. The
 * host shows its picker, keeps the bytes and answers [done] with what was
 * picked (the surface fires `upload {files}`).
 */
class FilePickRequest(
    private val model: at.exponential.ui.model.SurfaceModel,
    val componentId: String,
    val accept: String?,
    val multiple: Boolean,
) {
    /** VAPP-240: the surface that asked. */
    val surfaceId: String get() = model.id

    /** The picked files (main thread). */
    fun done(files: List<PickedFile>) = model.filesPicked(componentId, files)
}

/** Put `text` on the clipboard through [ExponentialUi.appContext]; a no-op without a context. */
internal fun systemCopy(text: String) {
    val ctx = ExponentialUi.appContext ?: return
    runCatching {
        val cm = ctx.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
        cm.setPrimaryClip(android.content.ClipData.newPlainText("code", text))
    }
}

/** Open `url` through the system (ACTION_VIEW on [ExponentialUi.appContext]); a no-op without a context. */
internal fun systemOpenUrl(url: String) {
    val ctx = ExponentialUi.appContext ?: return
    runCatching {
        ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/**
 * What an EMBEDDING APP provides. The SDK knows no transport: actions and
 * input edits are plain values the host forwards wherever it likes. Every
 * member has a default (the [NoHost] behaviour), so a host implements what
 * it needs. Called on the main thread.
 */
interface HostPlugin {
    /**
     * The icon registry: a catalog icon NAME (a registry concept such as
     * `nav-inbox`) → a composable drawn at `size` dp in the current content
     * colour. null = the placeholder circle.
     */
    fun icon(name: String, size: Float): (@Composable () -> Unit)? = null

    /** Round 9 §5.2, §6.8: a layout ran; the host sends what it left. */
    fun afterLayout(surfaceId: String) {}

    /** Every A2UI action. */
    fun onAction(event: SurfaceActionEvent) {}

    /** Host-owned text edits (debounced `change`, `commit` on blur / Enter). */
    fun onInput(event: SurfaceInputEvent) {}

    /**
     * A call to a host function (`functionCall` to a non-built-in name).
     * Default: nothing ([ExponentialHost] routes it through its policy).
     */
    fun onFunctionCall(event: SurfaceFunctionCallEvent) {}

    /**
     * Round 9 §6.8: an action with both: the function first, [then] (its
     * event) only when the function did not fail.
     */
    fun onFunctionCallThen(event: SurfaceFunctionCallEvent, then: () -> Unit) {
        onFunctionCall(event)
        then()
    }

    /**
     * The URL policy every href passes (`openUrl`, Link, markdown links,
     * FileUpload file urls) before [openUrl] sees it. null = the contract's
     * defaults (https, http, mailto, tel; relative urls against the media
     * `baseUrl`, else denied).
     */
    val urlPolicy: UrlPolicy? get() = null

    /**
     * The media policy every src passes (Image, Avatar, Video + poster,
     * Audio, markdown images) after [resolveUrl]. null = the contract's
     * defaults (https, http, data; no `file`).
     */
    val mediaOptions: MediaOptions? get() = null

    /**
     * Opens an href the URL policy ALLOWED (resolved, absolute). Default:
     * the system opener, through the policy again.
     */
    fun openUrl(url: String) {
        safeHref(this, url)?.let(::systemOpenUrl)
    }

    /**
     * #EXP-1674: the text of a `hostLinks` Text (the `Row` subtitle) as segments
     * that concatenate back to it; null = no links. eUI knows nothing of what
     * they mean.
     */
    fun textLinks(text: String): List<TextLinkSegment>? = null

    /**
     * #VAPP-258 (`catalog/host.json` `markdown`): a Markdown block image's
     * natural size, asked with the RAW src (null = unknown: the fixed box).
     * The box keeps its aspect, capped by the width and `imageMaxHeight`.
     * Must be pure: the measurer and the painter both ask.
     */
    fun imageSize(src: String): ImageSize? = null

    /** #VAPP-258: a press on a Markdown block image (a lightbox), with the RAW src; null = not pressable. */
    val onImagePress: ((src: String) -> Unit)? get() = null

    /**
     * #VAPP-258: a top-level Markdown paragraph that is one link, asked with
     * the RAW destination: `video` / `audio` / `file` + a name paints the
     * Video / AudioPlayer / FileUpload file row look (null = the paragraph).
     */
    fun attachment(href: String): HostAttachment? = null

    /** #VAPP-258 step 5 (`catalog/host.json` markdown.options): how this host's Markdowns render (the measurer reads the same). */
    val markdownOptions: at.exponential.ui.primitives.MarkdownOptions get() = at.exponential.ui.primitives.MarkdownOptions()

    /**
     * Rewrites a media src BEFORE the media policy (relative attachment
     * paths, signed URLs); its result still passes [mediaOptions], never
     * around it.
     */
    fun resolveUrl(src: String): String = src

    /**
     * VAPP-271 (`catalog/host.json` `media.appFile`): the bytes of an app
     * file for an `app-file:<fileId>` src (the Engine's runners: their
     * BlobStore), loaded once under the image limits and cached like any
     * picture. null (the default) = such a src loads nothing (the fallback).
     */
    val appFile: AppFileLoader? get() = null

    /**
     * `onPaintError` (`catalog/host.json` `paint`): a component's painter
     * failed; it paints an empty box. Called once per surface + component +
     * message.
     */
    fun onPaintError(error: PaintError) {}

    /** Called once per node id for each `Unknown` placeholder. */
    fun onUnknown(component: String, catalogId: String?, id: String) {}

    /**
     * A font family NAME a theme asks for → the Compose family to use (null
     * = the family registered via [ExponentialUi.registerFont], else the
     * default font).
     */
    fun fontFamily(name: String): FontFamily? = null

    /** A richer markdown renderer than the built-in one (null = built-in). */
    fun markdown(text: String, width: Float): (@Composable () -> Unit)? = null

    /** Put text on the clipboard (CodeBlock copy). Default: the system clipboard. */
    fun copy(text: String) = systemCopy(text)

    /**
     * A FileUpload wants files: show a picker, then `request.done(files)`.
     * Default: nothing (the SDK has no file access of its own).
     */
    fun pickFiles(request: FilePickRequest) {}

    /** #EDE-65 R: images pasted into a Composer (it fired `attach` {files}): read and upload the bytes. Default: nothing. */
    fun onUpload(event: UploadEvent) {}
}

/**
 * One piece of a host's [HostPlugin.textLinks] answer (`catalog/host.json`
 * urls). #VAPP-258: a Markdown paints a segment with an allowed href and a
 * valid [chip] as the Chip look, inline (a `hostLinks` Text paints its link).
 */
data class TextLinkSegment(val text: String, val href: String? = null, val chip: TextLinkChip? = null)

/** #VAPP-258: a text link's chip: [kind] = `issue` | `mention` | `entity`, a non-empty [label], an optional [icon] registry name and [ident] (painted mono before the label). */
data class TextLinkChip(val kind: String, val label: String, val icon: String? = null, val ident: String? = null)

/** #VAPP-258: a host's [HostPlugin.imageSize] answer (natural px; non-positive or non-finite = unknown). */
data class ImageSize(val width: Double, val height: Double)

/** #VAPP-258: a host's [HostPlugin.attachment] answer: [kind] = `video` | `audio` | `file`, a non-empty [name]. */
data class HostAttachment(val kind: String, val name: String)

/** The host that does nothing (previews, tests). */
object NoHost : HostPlugin

/** A host built from closures (the kitchen sink, quick embeddings). */
class ClosureHost(
    var icons: (String, Float) -> (@Composable () -> Unit)? = { _, _ -> null },
    var actions: (SurfaceActionEvent) -> Unit = {},
    var inputs: (SurfaceInputEvent) -> Unit = {},
    var urls: ((String) -> Unit)? = null,
    var unknowns: (String, String?, String) -> Unit = { _, _, _ -> },
    /** VAPP-271: `app-file:<fileId>` bytes (null = such a src loads nothing). */
    override val appFile: AppFileLoader? = null,
) : HostPlugin {
    override fun icon(name: String, size: Float): (@Composable () -> Unit)? = icons(name, size)
    override fun onAction(event: SurfaceActionEvent) = actions(event)
    override fun onInput(event: SurfaceInputEvent) = inputs(event)
    override fun openUrl(url: String) {
        val href = safeHref(this, url) ?: return
        val u = urls
        if (u != null) u(href) else systemOpenUrl(href)
    }
    override fun onUnknown(component: String, catalogId: String?, id: String) = unknowns(component, catalogId, id)
}
