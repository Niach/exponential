package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
private object OnboardingEmptyInput

@Singleton
class OnboardingApi @Inject constructor(private val trpc: TrpcClient) {
    // Marks onboarding complete for the current user (onboarding.complete in the
    // shared appRouter — sets users.onboardingCompletedAt = now). No input.
    suspend fun complete(accountId: String) {
        trpc.mutationUnit(
            accountId,
            path = "onboarding.complete",
            input = OnboardingEmptyInput,
            inputSerializer = OnboardingEmptyInput.serializer(),
        )
    }
}
