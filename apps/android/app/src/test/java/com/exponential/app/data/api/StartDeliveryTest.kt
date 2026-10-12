package com.exponential.app.data.api

import io.ktor.http.HttpStatusCode
import java.io.IOException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Recheck R3 (feedback-states.json `starting.maybeDelivered`): which failed
// starts may still have reached the device (toast + Retry + the quiet
// follow) and which were refused outright (toast + Retry only). iOS mirrors
// these cases (`StartDeliveryTests.swift`).
class StartDeliveryTest {

    @Test
    fun aTransportErrorMayHaveDelivered() {
        assertTrue(StartDelivery.maybeDelivered(SocketTimeoutException("read timed out")))
        assertTrue(StartDelivery.maybeDelivered(UnknownHostException("app.exponential.at")))
        assertTrue(StartDelivery.maybeDelivered(IOException("connection reset")))
        assertTrue(StartDelivery.maybeDelivered(RuntimeException("wrapped", IOException("closed"))))
    }

    @Test
    fun theRelayTimeoutMayHaveDelivered() {
        val relay = TrpcException(
            "Steer relay error (504)",
            HttpStatusCode.InternalServerError,
            code = "INTERNAL_SERVER_ERROR",
        )
        assertTrue(StartDelivery.maybeDelivered(relay))
        assertTrue(StartDelivery.maybeDelivered(504, "relay_timeout"))
    }

    @Test
    fun a5xxWithoutAnErrorBodyMayHaveDelivered() {
        assertTrue(StartDelivery.maybeDelivered(TrpcException("Request failed (HTTP 502)", HttpStatusCode.BadGateway)))
        assertTrue(StartDelivery.maybeDelivered(504, null))
    }

    @Test
    fun a4xxIsADefiniteFailure() {
        val refused = TrpcException(
            "That device is offline",
            HttpStatusCode.PreconditionFailed,
            code = "PRECONDITION_FAILED",
        )
        assertFalse(StartDelivery.maybeDelivered(refused))
        assertFalse(StartDelivery.maybeDelivered(TrpcException("Request failed (HTTP 400)", HttpStatusCode.BadRequest)))
    }

    @Test
    fun a5xxWithAnotherErrorIsADefiniteFailure() {
        val failed = TrpcException(
            "Steer relay error (502)",
            HttpStatusCode.InternalServerError,
            code = "INTERNAL_SERVER_ERROR",
        )
        assertFalse(StartDelivery.maybeDelivered(failed))
    }

    @Test
    fun aNeverSentRequestIsADefiniteFailure() {
        assertFalse(StartDelivery.maybeDelivered(TrpcException("No instance URL for account a1")))
        assertFalse(StartDelivery.maybeDelivered(IllegalStateException("no device")))
    }
}
