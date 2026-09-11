package io.github.luckylee6666.cascade;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class CascadeClientTest {

    @Test
    void parsesCanonicalUrl() {
        CascadeClient client = CascadeClient.fromUrl(
                "cascade://192.168.1.23:7071/project/proj-1?env=prod&token=t0k&reveal=1");
        assertEquals("http://192.168.1.23:7071", client.server());
        assertEquals("http://192.168.1.23:7071/api/projects/proj-1/resolved?env=prod&reveal=true",
                client.resolvedUrl());
        assertTrue(client.reveal());
        assertTrue(client.hasToken());
    }

    @Test
    void parsesShorthandAndLegacyScheme() {
        CascadeClient shorthand = CascadeClient.fromUrl("cascade://project/abc");
        assertEquals("http://localhost:7070/api/projects/abc/resolved", shorthand.resolvedUrl());

        CascadeClient legacy = CascadeClient.fromUrl("cc://project/abc");
        assertEquals("http://localhost:7070/api/projects/abc/resolved", legacy.resolvedUrl());
    }

    @Test
    void appliesDefaultPort() {
        CascadeClient client = CascadeClient.fromUrl("cascade://cfg.example.com/project/x");
        assertEquals("http://cfg.example.com:7070/api/projects/x/resolved", client.resolvedUrl());
    }

    @Test
    void urlEncodesEnv() {
        CascadeClient client = CascadeClient.fromUrl("cascade://project/x").withEnv("staging eu");
        assertEquals("http://localhost:7070/api/projects/x/resolved?env=staging%20eu",
                client.resolvedUrl());
    }

    @Test
    void rejectsGarbage() {
        assertThrows(IllegalArgumentException.class, () -> CascadeClient.fromUrl("not a url"));
        assertThrows(IllegalArgumentException.class, () -> CascadeClient.fromUrl("cascade://project/"));
    }

    @Test
    void watchHandleIsClosable() {
        WatchHandle handle = new WatchHandle();
        assertFalse(handle.isStopped());
        handle.close();
        assertTrue(handle.isStopped());
    }
}
