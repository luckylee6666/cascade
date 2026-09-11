package io.github.luckylee6666.cascade;

/**
 * An effective config value after env-chain resolution.
 *
 * @param value {@code null} for secrets that were not revealed.
 * @param source which layer produced the value: {@code base} / {@code env:<name>} / {@code project:<name>}.
 */
public record ResolvedConfig(
        String id,
        String key,
        String value,
        boolean secret,
        String source,
        String group,
        String description) {
}
