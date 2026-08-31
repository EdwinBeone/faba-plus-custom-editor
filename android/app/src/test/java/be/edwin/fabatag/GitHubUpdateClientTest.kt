package be.edwin.fabatag

import org.junit.Assert.assertEquals
import org.junit.Test

class GitHubUpdateClientTest {
    @Test
    fun comparesSemanticVersionsNumerically() {
        assertEquals(1, GitHubUpdateClient.compareVersions("0.10.0", "0.9.9"))
        assertEquals(-1, GitHubUpdateClient.compareVersions("0.4.9", "0.5.0"))
        assertEquals(0, GitHubUpdateClient.compareVersions("v1.2.3", "1.2.3"))
    }
}
