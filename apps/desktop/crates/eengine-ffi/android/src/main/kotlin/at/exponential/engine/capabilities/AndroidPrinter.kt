// #VAPP-265: print on Android: PrintManager with a PrintDocumentAdapter that
// writes the PDF's bytes as they are, or draws a PNG/JPEG fitted onto one
// page (PrintedPdfDocument). `printed` = the job's state once the dialog
// closed: queued, started, blocked or completed (Save as PDF completes) =
// true; cancelled, failed or never confirmed = false.
package at.exponential.engine.capabilities

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.RectF
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.PrintJob
import android.print.PrintJobInfo
import android.print.PrintManager
import android.print.pdf.PrintedPdfDocument
import java.io.FileOutputStream

class AndroidPrinter(private val host: ActivityHost) : Printer {
    override fun print(jobName: String, file: NativeFile): Boolean {
        val job: PrintJob = host.await { activity, done ->
            val manager = activity.getSystemService(Context.PRINT_SERVICE) as PrintManager
            var printJob: PrintJob? = null
            val adapter = BytesPrintAdapter(activity, jobName, file) {
                // onFinish: the print UI is gone.
                printJob?.let { done(Result.success(it)) }
            }
            printJob = manager.print(jobName, adapter, PrintAttributes.Builder().build())
        }
        repeat(STATE_POLLS) {
            val state: Int = host.await { _, done -> done(Result.success(job.info.state)) }
            if (state != PrintJobInfo.STATE_CREATED) return state in PRINTED
            Thread.sleep(STATE_POLL_MS)
        }
        return false
    }

    companion object {
        private const val STATE_POLLS = 10
        private const val STATE_POLL_MS = 200L
        private val PRINTED = setOf(PrintJobInfo.STATE_QUEUED, PrintJobInfo.STATE_STARTED, PrintJobInfo.STATE_BLOCKED, PrintJobInfo.STATE_COMPLETED)
    }
}

/** Writes one PDF / PNG / JPEG to the print system. */
private class BytesPrintAdapter(
    private val context: Context,
    private val name: String,
    private val file: NativeFile,
    private val finished: () -> Unit,
) : PrintDocumentAdapter() {
    private var attributes: PrintAttributes? = null
    private val isPdf get() = file.mimeType == "application/pdf"

    override fun onLayout(oldAttributes: PrintAttributes?, newAttributes: PrintAttributes, cancel: CancellationSignal?, callback: LayoutResultCallback, extras: Bundle?) {
        if (cancel?.isCanceled == true) return callback.onLayoutCancelled()
        attributes = newAttributes
        val info = PrintDocumentInfo.Builder(name)
            .setContentType(if (isPdf) PrintDocumentInfo.CONTENT_TYPE_DOCUMENT else PrintDocumentInfo.CONTENT_TYPE_PHOTO)
            .setPageCount(if (isPdf) PrintDocumentInfo.PAGE_COUNT_UNKNOWN else 1)
            .build()
        callback.onLayoutFinished(info, newAttributes != oldAttributes)
    }

    override fun onWrite(pages: Array<out PageRange>?, destination: ParcelFileDescriptor, cancel: CancellationSignal?, callback: WriteResultCallback) {
        try {
            FileOutputStream(destination.fileDescriptor).use { out ->
                if (isPdf) out.write(file.bytes) else writeImage(out)
            }
            if (cancel?.isCanceled == true) callback.onWriteCancelled() else callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
        } catch (e: Exception) {
            callback.onWriteFailed(e.message)
        }
    }

    override fun onFinish() = finished()

    private fun writeImage(out: FileOutputStream) {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(file.bytes, 0, file.bytes.size, bounds)
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= MAX_SIDE || bounds.outHeight / (sample * 2) >= MAX_SIDE) sample *= 2
        val bitmap = BitmapFactory.decodeByteArray(file.bytes, 0, file.bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
            ?: throw IllegalArgumentException("not an image")
        val document = PrintedPdfDocument(context, attributes ?: PrintAttributes.Builder().build())
        try {
            val page = document.startPage(0)
            val area = page.info.contentRect
            val scale = minOf(area.width().toFloat() / bitmap.width, area.height().toFloat() / bitmap.height)
            val w = bitmap.width * scale
            val h = bitmap.height * scale
            val left = area.left + (area.width() - w) / 2
            val top = area.top + (area.height() - h) / 2
            page.canvas.drawBitmap(bitmap, null, RectF(left, top, left + w, top + h), null)
            document.finishPage(page)
            document.writeTo(out)
        } finally {
            document.close()
            bitmap.recycle()
        }
    }

    companion object {
        private const val MAX_SIDE = 2400
    }
}
