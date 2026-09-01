package be.edwin.fabatag

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NfcWriteSessionTest {
    @Test
    fun initialTagCanBeClaimedExactlyOnce() {
        val session = NfcWriteSession()

        assertTrue(session.acceptsReaderCallbacks())
        assertTrue(session.beginInspection())
        assertFalse(session.acceptsReaderCallbacks())
        assertFalse(session.beginInspection())
        assertEquals(NfcSessionPhase.INSPECTING, session.currentPhase())
    }

    @Test
    fun fourStepsCanOnlyAdvanceOnceAndInOrder() {
        val session = NfcWriteSession()

        assertFalse(session.beginWriting())
        assertTrue(session.beginInspection())
        assertTrue(session.beginClearing())
        assertFalse(session.beginClearing())
        assertTrue(session.beginWriting())
        assertTrue(session.beginInlineVerification())
        assertFalse(session.beginInlineVerification())
        assertTrue(session.finish())
        assertEquals(NfcSessionPhase.FINISHED, session.currentPhase())
    }

    @Test
    fun formattedTagGetsExactlyOneFreshVerification() {
        val session = NfcWriteSession()

        assertTrue(session.beginInspection())
        assertTrue(session.beginClearing())
        assertTrue(session.beginWriting())
        assertTrue(session.awaitFreshVerification())
        assertTrue(session.acceptsReaderCallbacks())
        assertTrue(session.beginVerification())
        assertFalse(session.acceptsReaderCallbacks())
        assertFalse(session.beginVerification())
        assertTrue(session.finish())
        assertEquals(NfcSessionPhase.FINISHED, session.currentPhase())
    }

    @Test
    fun completionAndCancellationAreTerminal() {
        val completed = NfcWriteSession()
        assertTrue(completed.beginInspection())
        assertTrue(completed.finish())
        assertFalse(completed.finish())
        assertFalse(completed.cancel())
        assertFalse(completed.beginInspection())

        val cancelled = NfcWriteSession()
        assertTrue(cancelled.cancel())
        assertFalse(cancelled.acceptsReaderCallbacks())
        assertFalse(cancelled.beginInspection())
        assertFalse(cancelled.finish())
    }

    @Test
    fun oldCompletedSessionCannotConsumeNewSessionCallback() {
        val oldSession = NfcWriteSession()
        assertTrue(oldSession.beginInspection())
        assertTrue(oldSession.finish())

        val newSession = NfcWriteSession()
        assertFalse(oldSession.beginInspection())
        assertTrue(newSession.beginInspection())
    }
}
