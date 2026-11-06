// #VAPP-265: `share` (send) and `print` (print). Android: the ACTION_SEND /
// ACTION_SEND_MULTIPLE chooser (a file through the library's own read-only
// ShareFileProvider; `shared` from the chooser's chosen-component callback)
// and PrintManager (a PDF as is, a PNG/JPEG drawn onto pages; `printed` from
// the print job's state). No permission.
package at.exponential.engine.capabilities

/** What the share sheet carries. */
class ShareContent(val title: String?, val text: String?, val file: NativeFile?)

/** The platform's share sheet (blocks until it closes): true = the person picked a target. */
fun interface ShareSheet {
    fun send(content: ShareContent): Boolean
}

/** The platform's print dialog (blocks until it closes): true = a job was queued. */
fun interface Printer {
    fun print(jobName: String, file: NativeFile): Boolean
}

class ShareCapability(
    private val sheet: ShareSheet,
    mainThread: MainThreadCheck = AndroidMainThread,
) : NativeCapability("share", mainThread) {
    override val toolNames = setOf("send")

    override fun handle(tool: String, args: Args): Map<String, Any?> {
        val file = args.file("file")
        val text = sharedText(args.optString("text"), args.optString("url"))
        if (text == null && file == null) throw ToolFailure.failed("nothing to share")
        return mapOf("shared" to sheet.send(ShareContent(args.optString("title"), text, file)))
    }

    companion object {
        /** EXTRA_TEXT: the text, the link on its own line after it. */
        fun sharedText(text: String?, url: String?): String? = listOfNotNull(text?.takeIf { it.isNotEmpty() }, url?.takeIf { it.isNotEmpty() })
            .joinToString("\n").takeIf { it.isNotEmpty() }
    }
}

class PrintCapability(
    private val printer: Printer,
    mainThread: MainThreadCheck = AndroidMainThread,
) : NativeCapability("print", mainThread) {
    override val toolNames = setOf("print")

    override fun handle(tool: String, args: Args): Map<String, Any?> {
        val file = args.asFile()
        if (file.mimeType !in PRINTABLE) throw ToolFailure.failed("unsupported file")
        return mapOf("printed" to printer.print(Text.name(file.name, "document"), file))
    }

    companion object {
        val PRINTABLE = setOf("application/pdf", "image/png", "image/jpeg")
    }
}
