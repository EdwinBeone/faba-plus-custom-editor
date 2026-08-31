package be.edwin.fabatag

import java.util.concurrent.atomic.AtomicBoolean

/** Allows exactly one NFC reader callback for each explicit user action. */
internal class NfcWriteGate {
    private val consumed = AtomicBoolean(true)

    fun arm() = consumed.set(false)

    fun tryConsume(): Boolean = consumed.compareAndSet(false, true)

    fun cancel() = consumed.set(true)

    fun isArmed(): Boolean = !consumed.get()
}
