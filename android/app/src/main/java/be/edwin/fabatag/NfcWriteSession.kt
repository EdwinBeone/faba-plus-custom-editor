package be.edwin.fabatag

import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

internal enum class NfcSessionPhase {
    WAITING_FOR_TAG,
    INSPECTING,
    CLEARING,
    WRITING,
    WAITING_FOR_VERIFICATION,
    VERIFYING,
    FINISHED,
    CANCELLED,
}

/**
 * State machine for one explicit tap on the "write NFC tag" button.
 *
 * Each NFC reader callback receives the session instance it was created for.
 * A delayed callback from an older reader can therefore never claim a newer
 * operation, and each discovery/verification transition is atomic.
 */
internal class NfcWriteSession {
    val id: Long = nextId.incrementAndGet()

    private val phase = AtomicReference(NfcSessionPhase.WAITING_FOR_TAG)

    fun beginInspection(): Boolean =
        phase.compareAndSet(NfcSessionPhase.WAITING_FOR_TAG, NfcSessionPhase.INSPECTING)

    fun beginClearing(): Boolean =
        phase.compareAndSet(NfcSessionPhase.INSPECTING, NfcSessionPhase.CLEARING)

    fun beginWriting(): Boolean =
        phase.compareAndSet(NfcSessionPhase.CLEARING, NfcSessionPhase.WRITING)

    fun beginInlineVerification(): Boolean =
        phase.compareAndSet(NfcSessionPhase.WRITING, NfcSessionPhase.VERIFYING)

    fun awaitFreshVerification(): Boolean =
        phase.compareAndSet(NfcSessionPhase.WRITING, NfcSessionPhase.WAITING_FOR_VERIFICATION)

    fun beginVerification(): Boolean =
        phase.compareAndSet(NfcSessionPhase.WAITING_FOR_VERIFICATION, NfcSessionPhase.VERIFYING)

    fun finish(): Boolean = moveToTerminal(NfcSessionPhase.FINISHED)

    fun cancel(): Boolean = moveToTerminal(NfcSessionPhase.CANCELLED)

    fun acceptsReaderCallbacks(): Boolean = when (phase.get()) {
        NfcSessionPhase.WAITING_FOR_TAG,
        NfcSessionPhase.WAITING_FOR_VERIFICATION,
        -> true

        else -> false
    }

    fun currentPhase(): NfcSessionPhase = phase.get()

    private fun moveToTerminal(terminal: NfcSessionPhase): Boolean {
        while (true) {
            val current = phase.get()
            if (current == NfcSessionPhase.FINISHED || current == NfcSessionPhase.CANCELLED) return false
            if (phase.compareAndSet(current, terminal)) return true
        }
    }

    private companion object {
        val nextId = AtomicLong(0)
    }
}
