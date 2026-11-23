package com.exponential.app.domain

import at.exponential.ui.ffi.ExpressDecoder
import at.exponential.ui.ffi.expressUnwrapJson

/**
 * #EXP-2073: [ChatExpress.Sdk] over the eui FFI (THE SDK every platform
 * calls, never a re-implementation): `expressUnwrapJson` splits, an
 * UNWRAPPED `ExpressDecoder` (surface id = the block key) compiles.
 */
object ChatExpressFfi : ChatExpress.Sdk {
    override fun unwrap(text: String): List<ChatExpress.RawPart> = ChatExpress.parseUnwrap(expressUnwrapJson(text))

    override fun decoder(surfaceId: String): ChatExpress.Decoder = object : ChatExpress.Decoder {
        private val d = ExpressDecoder(false, surfaceId)

        override fun push(chunk: String): ChatExpress.Decoded = ChatExpress.parseDecoded(d.push(chunk))

        override fun end(): ChatExpress.Decoded = ChatExpress.parseDecoded(d.end()).also { d.close() }
    }
}
