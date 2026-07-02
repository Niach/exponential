package com.exponential.app.data.api

import com.exponential.app.BuildConfig
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.DefaultRequest
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.logging.LogLevel
import io.ktor.client.plugins.logging.Logger
import io.ktor.client.plugins.logging.Logging
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.request.header
import io.ktor.serialization.kotlinx.json.json
import javax.inject.Singleton
import kotlinx.serialization.json.Json

@Module
@InstallIn(SingletonComponent::class)
object HttpClientModule {

    @Provides
    @Singleton
    fun provideJson(): Json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Provides
    @Singleton
    fun provideHttpClient(json: Json): HttpClient =
        HttpClient(CIO) {
            expectSuccess = false
            install(ContentNegotiation) { json(json) }
            // Steer viewer sockets (relay PTY mirror, masterplan §5c) ride the
            // same client; the plugin is inert for plain HTTP calls.
            install(WebSockets)
            if (BuildConfig.DEBUG) {
                install(Logging) {
                    level = LogLevel.INFO
                    logger = object : Logger {
                        override fun log(message: String) {
                            android.util.Log.d("ktor", message)
                        }
                    }
                }
            }
            install(DefaultRequest) {
                header("Accept", "application/json")
            }
        }
}
