package be.edwin.fabatag

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NfcWriteGateTest {
    @Test
    fun oneArmConsumesExactlyOneTag() {
        val gate = NfcWriteGate()

        assertFalse(gate.tryConsume())
        gate.arm()
        assertTrue(gate.isArmed())
        assertTrue(gate.tryConsume())
        assertFalse(gate.isArmed())
        assertFalse(gate.tryConsume())

        gate.arm()
        assertTrue(gate.tryConsume())
    }

    @Test
    fun cancellationClosesTheSession() {
        val gate = NfcWriteGate()
        gate.arm()
        gate.cancel()
        assertFalse(gate.tryConsume())
    }
}
