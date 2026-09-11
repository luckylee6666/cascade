import { Config, ChangeCallback } from './models';
interface ResolvedConfig {
    id: string;
    key: string;
    value: string | null;
    secret: boolean;
    source: string;
    group: string | null;
    description: string | null;
}
/**
 * Cascade SDK — HTTP + SSE client with snapshot fallback.
 *
 * Secrets come back as null unless `reveal` is enabled AND the token has
 * admin permission. The snapshot stores exactly what the server returned.
 */
export declare class ConfigCenter {
    private server;
    private projectId;
    private token;
    private env;
    private reveal;
    private snapshotPath;
    private callbacks;
    private cache;
    private cacheTs;
    private watching;
    constructor(server: string, projectId: string, token?: string, env?: string | null, reveal?: boolean, snapshotPath?: string | null);
    /**
     * Canonical:  cc://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]
     * Shorthand:  cc://project/<id>   (server defaults to http://localhost:7070)
     */
    static fromUrl(url: string): ConfigCenter;
    private headers;
    private resolvedUrl;
    private fetchResolved;
    private writeSnapshot;
    private readSnapshot;
    /** Cached briefly; falls back to the last snapshot when the server is down. */
    private data;
    get(key: string, fallback?: string | null): Promise<string | null>;
    listConfigs(group?: string): Promise<Config[]>;
    /** Force a fetch (or snapshot fallback) and return resolved configs. */
    refresh(): Promise<ResolvedConfig[]>;
    set(key: string, value: string, secret?: boolean): Promise<Config>;
    delete(id: string): Promise<void>;
    on(event: 'change', callback: ChangeCallback): void;
    /** Start the SSE listener with auto-reconnect (Node 18+ fetch). */
    startWatch(): void;
    private watchLoop;
    private exportPairs;
    exportFile(filePath: string, format?: 'yaml' | 'json'): Promise<void>;
    exportEnv(filePath: string): Promise<void>;
}
export {};
//# sourceMappingURL=client.d.ts.map