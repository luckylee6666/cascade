package io.github.luckylee6666.cascade;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonObject;

import java.io.IOException;
import java.net.URI;
import java.net.URLDecoder;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.Consumer;

/**
 * Cascade SDK — HTTP + SSE client with snapshot fallback.
 *
 * <pre>{@code
 * CascadeClient cc = CascadeClient.fromUrl(
 *     "cascade://localhost:7070/project/<id>?env=prod&token=xxx");
 * System.out.println(cc.get("database.host").orElse(""));
 *
 * // Change subscription (SSE, auto-reconnect) on a daemon thread.
 * WatchHandle handle = cc.watch(event -> System.out.println("changed: " + event));
 * }</pre>
 *
 * Secrets come back as {@code Optional.empty()} / {@code null} unless
 * {@link #withReveal(boolean)} is set AND the token has admin permission.
 * The snapshot file stores exactly what the server returned.
 */
public final class CascadeClient {

    public enum Format {
        YAML, JSON, DOTENV
    }

    private static final Gson GSON = new Gson();
    private static final Gson PRETTY = new GsonBuilder().setPrettyPrinting().create();
    private static final Duration CACHE_TTL = Duration.ofSeconds(5);
    private static final long RECONNECT_MILLIS = 3000;
    private static final int DEFAULT_PORT = 7070;

    private final String server;
    private final String projectId;
    private final HttpClient http;
    private String token;
    private String env;
    private boolean reveal;
    private Path snapshotPath;

    private volatile List<ResolvedConfig> cache;
    private volatile Instant cacheAt;

    private CascadeClient(String server, String projectId) {
        this.server = server.endsWith("/") ? server.substring(0, server.length() - 1) : server;
        this.projectId = projectId;
        this.http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(10)).build();
        this.snapshotPath = Path.of(
                System.getProperty("user.home"), ".cascade", "sdk", projectId + ".json");
    }

    /**
     * Parse a project link.
     *
     * <p>Canonical: {@code cascade://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]}
     * <br>Shorthand: {@code cascade://project/<id>} (server defaults to {@code http://localhost:7070})
     * <br>Legacy {@code cc://} links parse identically (the scheme is ignored).
     */
    public static CascadeClient fromUrl(String url) {
        URI uri;
        try {
            uri = URI.create(url);
        } catch (IllegalArgumentException e) {
            throw new IllegalArgumentException("cannot parse url: " + url, e);
        }

        String token = null;
        String env = null;
        boolean reveal = false;
        String query = uri.getRawQuery();
        if (query != null) {
            for (String pair : query.split("&")) {
                int i = pair.indexOf('=');
                if (i < 0) {
                    continue;
                }
                String key = URLDecoder.decode(pair.substring(0, i), StandardCharsets.UTF_8);
                String value = URLDecoder.decode(pair.substring(i + 1), StandardCharsets.UTF_8);
                switch (key) {
                    case "token" -> token = value;
                    case "env" -> env = value;
                    case "reveal" -> reveal = isTrue(value);
                    default -> { }
                }
            }
        }

        String host = uri.getHost();
        String server;
        String projectId;
        if (host == null || host.isEmpty() || host.equals("project")) {
            server = "http://localhost:" + DEFAULT_PORT;
            projectId = lastPathSegment(uri.getPath());
        } else {
            int port = uri.getPort() > 0 ? uri.getPort() : DEFAULT_PORT;
            server = "http://" + host + ":" + port;
            String path = uri.getPath() == null ? "" : uri.getPath();
            projectId = path.replaceFirst("^/project/", "");
            projectId = projectId.replaceAll("^/+", "").replaceAll("/+$", "");
        }
        if (projectId.isEmpty()) {
            throw new IllegalArgumentException("cannot parse project id from " + url);
        }

        CascadeClient client = new CascadeClient(server, projectId);
        client.token = token;
        client.env = env;
        client.reveal = reveal;
        return client;
    }

    public CascadeClient withToken(String token) {
        this.token = token;
        return this;
    }

    public CascadeClient withEnv(String env) {
        this.env = env;
        return this;
    }

    /** Reveal secret values (requires an admin token on the server). */
    public CascadeClient withReveal(boolean reveal) {
        this.reveal = reveal;
        return this;
    }

    public CascadeClient withSnapshotPath(Path path) {
        this.snapshotPath = path;
        return this;
    }

    // ── reads ───────────────────────────────────────────────

    /**
     * Effective value for this project/env.
     * Empty when the key is missing or its secret is hidden.
     */
    public Optional<String> get(String key) throws IOException, InterruptedException {
        for (ResolvedConfig c : data(false)) {
            if (c.key().equals(key)) {
                return Optional.ofNullable(c.value());
            }
        }
        return Optional.empty();
    }

    public String getOr(String key, String fallback) throws IOException, InterruptedException {
        return get(key).orElse(fallback);
    }

    /** All resolved configs (briefly cached, snapshot fallback on failure). */
    public List<ResolvedConfig> list() throws IOException, InterruptedException {
        return data(false);
    }

    /** Force a fetch, falling back to the snapshot when the server is down. */
    public List<ResolvedConfig> refresh() throws IOException, InterruptedException {
        return data(true);
    }

    // ── writes ──────────────────────────────────────────────

    /** Create or update a raw config entry (requires an admin token). */
    public void set(String key, String value, boolean secret)
            throws IOException, InterruptedException {
        JsonObject body = new JsonObject();
        body.addProperty("key", key);
        body.addProperty("value", value);
        body.addProperty("secret", secret);

        HttpRequest.Builder builder = HttpRequest.newBuilder(URI.create(server + "/api/configs"))
                .timeout(Duration.ofSeconds(30))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(GSON.toJson(body)));
        if (token != null && !token.isEmpty()) {
            builder.header("Authorization", "Bearer " + token);
        }
        HttpResponse<String> response =
                http.send(builder.build(), HttpResponse.BodyHandlers.ofString());
        if (response.statusCode() != 200) {
            throw new IOException("HTTP " + response.statusCode());
        }
        cache = null;
    }

    // ── realtime ────────────────────────────────────────────

    /**
     * Stream SSE change events on a daemon thread with auto-reconnect.
     * The callback receives the raw event, e.g. {@code "config_updated:<id>"}.
     */
    public WatchHandle watch(Consumer<String> callback) {
        WatchHandle handle = new WatchHandle();
        Thread thread = new Thread(() -> {
            while (!handle.isStopped()) {
                try {
                    HttpRequest.Builder builder =
                            HttpRequest.newBuilder(URI.create(server + "/api/sse/configs"))
                                    .header("Accept", "text/event-stream")
                                    .GET();
                    if (token != null && !token.isEmpty()) {
                        builder.header("Authorization", "Bearer " + token);
                    }
                    HttpResponse<java.util.stream.Stream<String>> response =
                            http.send(builder.build(), HttpResponse.BodyHandlers.ofLines());
                    if (response.statusCode() == 200) {
                        try (var lines = response.body()) {
                            lines.forEach(line -> {
                                if (line.startsWith("data:")) {
                                    callback.accept(line.substring("data:".length()).trim());
                                }
                            });
                        }
                    }
                } catch (Exception ignored) {
                    // reconnect below
                }
                if (handle.isStopped()) {
                    return;
                }
                try {
                    Thread.sleep(RECONNECT_MILLIS);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    return;
                }
            }
        }, "cascade-watch");
        thread.setDaemon(true);
        thread.start();
        return handle;
    }

    // ── exports ─────────────────────────────────────────────

    public void exportEnv(Path path) throws IOException, InterruptedException {
        exportFile(path, Format.DOTENV);
    }

    public void exportFile(Path path, Format format) throws IOException, InterruptedException {
        Map<String, String> pairs = exportPairs();
        String content = switch (format) {
            case JSON -> PRETTY.toJson(pairs);
            case YAML -> pairs.entrySet().stream()
                    .map(e -> e.getKey() + ": \"" + e.getValue().replace("\"", "\\\"") + "\"")
                    .reduce((a, b) -> a + "\n" + b)
                    .orElse("");
            case DOTENV -> dotenv(pairs);
        };
        Files.writeString(path, content, StandardCharsets.UTF_8);
    }

    /** Path of the snapshot file used as an offline fallback. */
    public Path snapshotPath() {
        return snapshotPath;
    }

    // ── test hooks (package-private) ────────────────────────

    String server() {
        return server;
    }

    boolean reveal() {
        return reveal;
    }

    boolean hasToken() {
        return token != null;
    }

    // ── internals ───────────────────────────────────────────

    String resolvedUrl() {
        StringBuilder url = new StringBuilder(server)
                .append("/api/projects/").append(projectId).append("/resolved");
        List<String> params = new ArrayList<>();
        if (env != null && !env.isEmpty()) {
            params.add("env=" + urlEncode(env));
        }
        if (reveal) {
            params.add("reveal=true");
        }
        if (!params.isEmpty()) {
            url.append('?').append(String.join("&", params));
        }
        return url.toString();
    }

    private List<ResolvedConfig> fetch() throws IOException, InterruptedException {
        String url = resolvedUrl();
        HttpRequest.Builder builder = HttpRequest.newBuilder(URI.create(url))
                .timeout(Duration.ofSeconds(30))
                .header("Accept", "application/json")
                .GET();
        if (token != null && !token.isEmpty()) {
            builder.header("Authorization", "Bearer " + token);
        }
        HttpResponse<String> response =
                http.send(builder.build(), HttpResponse.BodyHandlers.ofString());
        if (response.statusCode() != 200) {
            throw new IOException("HTTP " + response.statusCode() + " from " + url);
        }
        ResolvedConfig[] parsed = GSON.fromJson(response.body(), ResolvedConfig[].class);
        List<ResolvedConfig> data = parsed == null ? List.of() : Arrays.asList(parsed);
        writeSnapshot(data);
        cache = data;
        cacheAt = Instant.now();
        return data;
    }

    private List<ResolvedConfig> data(boolean force) throws IOException, InterruptedException {
        if (!force && cache != null && cacheAt != null
                && Instant.now().isBefore(cacheAt.plus(CACHE_TTL))) {
            return cache;
        }
        try {
            return fetch();
        } catch (IOException fetchError) {
            List<ResolvedConfig> snapshot = readSnapshot();
            if (snapshot != null) {
                cache = snapshot;
                cacheAt = Instant.now();
                return snapshot;
            }
            throw fetchError;
        }
    }

    private void writeSnapshot(List<ResolvedConfig> data) {
        try {
            Path parent = snapshotPath.getParent();
            if (parent != null) {
                Files.createDirectories(parent);
            }
            Files.writeString(snapshotPath, GSON.toJson(data), StandardCharsets.UTF_8);
        } catch (IOException ignored) {
            // best effort
        }
    }

    private List<ResolvedConfig> readSnapshot() {
        try {
            ResolvedConfig[] parsed =
                    GSON.fromJson(Files.readString(snapshotPath), ResolvedConfig[].class);
            return parsed == null ? null : Arrays.asList(parsed);
        } catch (Exception e) {
            return null;
        }
    }

    private Map<String, String> exportPairs() throws IOException, InterruptedException {
        Map<String, String> out = new LinkedHashMap<>();
        for (ResolvedConfig c : refresh()) {
            String value = c.value() != null
                    ? c.value()
                    : "${" + c.key().replace('.', '_').replace('-', '_').toUpperCase() + "}";
            out.put(c.key(), value);
        }
        return out;
    }

    private static String dotenv(Map<String, String> pairs) {
        StringBuilder sb = new StringBuilder("# generated by cascade\n");
        for (Map.Entry<String, String> e : pairs.entrySet()) {
            String name = e.getKey().replace('.', '_').replace('-', '_').toUpperCase();
            String escaped = e.getValue()
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n");
            sb.append(name).append("=\"").append(escaped).append('"').append('\n');
        }
        return sb.toString();
    }

    private static String lastPathSegment(String path) {
        if (path == null) {
            return "";
        }
        String[] segments = path.split("/");
        for (int i = segments.length - 1; i >= 0; i--) {
            if (!segments[i].isEmpty()) {
                return segments[i];
            }
        }
        return "";
    }

    private static boolean isTrue(String value) {
        return value.equalsIgnoreCase("1")
                || value.equalsIgnoreCase("true")
                || value.equalsIgnoreCase("yes");
    }

    private static String urlEncode(String value) {
        return URLEncoder.encode(value, StandardCharsets.UTF_8).replace("+", "%20");
    }
}
