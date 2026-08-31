package be.edwin.fabatag

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PayloadValidatorTest {
    @Test
    fun acceptsOnlyMatchingCustomFabaPlusPayloads() {
        assertTrue(PayloadValidator.isValid(playlist("3101", "02190530310100")))
        assertFalse(PayloadValidator.isValid(playlist("0001", "02190530000100")))
        assertFalse(PayloadValidator.isValid(playlist("3101", "02190530310200")))
        assertFalse(PayloadValidator.isValid(playlist("9000", "02190530900000")))
    }

    private fun playlist(id: String, payload: String) = CloudPlaylist(
        figureId = id,
        name = "Test",
        nfcPayload = payload,
        trackCount = 1,
        tracks = listOf(CloudTrack(0, "Piste 1", true, 100, "a".repeat(64))),
    )
}
