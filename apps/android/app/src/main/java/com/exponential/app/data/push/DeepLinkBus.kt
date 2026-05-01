package com.exponential.app.data.push

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * One-shot bridge from MainActivity (which receives Android intents) to
 * the Compose nav graph (which decides how to react). Stores the most
 * recent target; the consumer must call consume() after acting on it.
 */
@Singleton
class DeepLinkBus @Inject constructor() {
    sealed interface Target {
        data class Issue(val id: String) : Target
    }

    private val _target = MutableStateFlow<Target?>(null)
    val target: StateFlow<Target?> = _target.asStateFlow()

    fun openIssue(id: String) {
        _target.value = Target.Issue(id)
    }

    fun consume() {
        _target.value = null
    }
}
