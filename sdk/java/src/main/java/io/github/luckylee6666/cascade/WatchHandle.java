package io.github.luckylee6666.cascade;

import java.util.concurrent.atomic.AtomicBoolean;

/** Handle returned by {@link CascadeClient#watch}; stops the listener on close. */
public final class WatchHandle implements AutoCloseable {

    private final AtomicBoolean stopped = new AtomicBoolean(false);

    public void stop() {
        stopped.set(true);
    }

    public boolean isStopped() {
        return stopped.get();
    }

    @Override
    public void close() {
        stop();
    }
}
