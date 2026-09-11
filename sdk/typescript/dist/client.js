"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ConfigCenter = void 0;
const fs = require('fs');
const path = require('path');
const os = require('os');
/**
 * Cascade SDK — HTTP + SSE client with snapshot fallback.
 *
 * Secrets come back as null unless `reveal` is enabled AND the token has
 * admin permission. The snapshot stores exactly what the server returned.
 */
class ConfigCenter {
    constructor(server, projectId, token = '', env = null, reveal = false, snapshotPath = null) {
        this.callbacks = [];
        this.cache = null;
        this.cacheTs = 0;
        this.watching = false;
        this.server = server.replace(/\/+$/, '');
        this.projectId = projectId;
        this.token = token;
        this.env = env;
        this.reveal = reveal;
        this.snapshotPath =
            snapshotPath || path.join(os.homedir(), '.cc-sdk', `${projectId}.json`);
    }
    /**
     * Canonical:  cc://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]
     * Shorthand:  cc://project/<id>   (server defaults to http://localhost:7070)
     */
    static fromUrl(url) {
        const parsed = new URL(url);
        const token = parsed.searchParams.get('token') || '';
        const env = parsed.searchParams.get('env');
        const reveal = ['1', 'true', 'yes'].includes((parsed.searchParams.get('reveal') || '').toLowerCase());
        let server;
        let projectId;
        if (parsed.host === 'project' || parsed.host === '') {
            server = 'http://localhost:7070';
            const segs = parsed.pathname.split('/').filter(Boolean);
            projectId = segs[segs.length - 1] || '';
        }
        else {
            server = `http://${parsed.hostname}:${parsed.port || '7070'}`;
            projectId = parsed.pathname.replace('/project/', '').replace(/^\/+|\/+$/g, '');
        }
        if (!projectId) {
            throw new Error(`cannot parse project id from ${url}`);
        }
        return new ConfigCenter(server, projectId, token, env, reveal);
    }
    // ── internals ───────────────────────────────────────────────
    headers() {
        const headers = {
            'Content-Type': 'application/json',
        };
        if (this.token) {
            headers['Authorization'] = `Bearer ${this.token}`;
        }
        return headers;
    }
    resolvedUrl() {
        const u = new URL(`${this.server}/api/projects/${this.projectId}/resolved`);
        if (this.env)
            u.searchParams.set('env', this.env);
        if (this.reveal)
            u.searchParams.set('reveal', 'true');
        return u.toString();
    }
    async fetchResolved() {
        const response = await fetch(this.resolvedUrl(), { headers: this.headers() });
        if (!response.ok)
            throw new Error(`HTTP ${response.status}`);
        const data = (await response.json());
        this.writeSnapshot(data);
        this.cache = data;
        this.cacheTs = Date.now();
        return data;
    }
    writeSnapshot(data) {
        try {
            fs.mkdirSync(path.dirname(this.snapshotPath), { recursive: true });
            fs.writeFileSync(this.snapshotPath, JSON.stringify(data));
        }
        catch (e) {
            /* snapshot is best-effort */
        }
    }
    readSnapshot() {
        try {
            return JSON.parse(fs.readFileSync(this.snapshotPath, 'utf8'));
        }
        catch (e) {
            return [];
        }
    }
    /** Cached briefly; falls back to the last snapshot when the server is down. */
    async data(maxAgeMs = 5000) {
        if (this.cache && Date.now() - this.cacheTs < maxAgeMs) {
            return this.cache;
        }
        try {
            return await this.fetchResolved();
        }
        catch (e) {
            const snap = this.readSnapshot();
            this.cache = snap;
            this.cacheTs = Date.now();
            return snap;
        }
    }
    // ── reads ───────────────────────────────────────────────────
    async get(key, fallback = null) {
        for (const c of await this.data()) {
            if (c.key === key) {
                return c.value !== null ? c.value : fallback;
            }
        }
        return fallback;
    }
    async listConfigs(group) {
        const all = await this.data();
        return group ? all.filter(c => c.group === group) : all;
    }
    /** Force a fetch (or snapshot fallback) and return resolved configs. */
    async refresh() {
        try {
            return await this.fetchResolved();
        }
        catch (e) {
            const snap = this.readSnapshot();
            this.cache = snap;
            this.cacheTs = Date.now();
            return snap;
        }
    }
    // ── writes ──────────────────────────────────────────────────
    async set(key, value, secret = false) {
        const response = await fetch(`${this.server}/api/configs`, {
            method: 'POST',
            headers: this.headers(),
            body: JSON.stringify({ key, value, secret }),
        });
        if (!response.ok)
            throw new Error(`HTTP ${response.status}`);
        this.cache = null;
        return (await response.json());
    }
    async delete(id) {
        const response = await fetch(`${this.server}/api/configs/${id}`, {
            method: 'DELETE',
            headers: this.headers(),
        });
        if (!response.ok)
            throw new Error(`HTTP ${response.status}`);
        this.cache = null;
    }
    // ── realtime (SSE) ──────────────────────────────────────────
    on(event, callback) {
        if (event === 'change')
            this.callbacks.push(callback);
    }
    /** Start the SSE listener with auto-reconnect (Node 18+ fetch). */
    startWatch() {
        if (this.watching)
            return;
        this.watching = true;
        void this.watchLoop();
    }
    async watchLoop() {
        for (;;) {
            try {
                const res = await fetch(`${this.server}/api/sse/configs`, {
                    headers: this.headers(),
                });
                if (!res.ok || !res.body)
                    throw new Error(`HTTP ${res.status}`);
                const reader = res.body.getReader();
                const decoder = new TextDecoder();
                let buf = '';
                for (;;) {
                    const { value, done } = await reader.read();
                    if (done)
                        break;
                    buf += decoder.decode(value, { stream: true });
                    const lines = buf.split('\n');
                    buf = lines.pop() || '';
                    for (const line of lines) {
                        if (!line.startsWith('data:'))
                            continue;
                        const event = line.slice(5).trim();
                        this.cache = null;
                        for (const cb of [...this.callbacks]) {
                            try {
                                cb(event);
                            }
                            catch (e) {
                                /* keep streaming */
                            }
                        }
                    }
                }
            }
            catch (e) {
                /* reconnect below */
            }
            await new Promise(r => setTimeout(r, 3000));
        }
    }
    // ── exports ─────────────────────────────────────────────────
    async exportPairs() {
        const out = {};
        for (const c of await this.refresh()) {
            out[c.key] =
                c.value !== null
                    ? c.value
                    : '${' + c.key.replace(/\./g, '_').toUpperCase() + '}';
        }
        return out;
    }
    async exportFile(filePath, format = 'yaml') {
        const data = await this.exportPairs();
        let content;
        if (format === 'json') {
            content = JSON.stringify(data, null, 2);
        }
        else {
            content = Object.entries(data)
                .map(([k, v]) => `${k}: "${String(v).replace(/"/g, '\\"')}"`)
                .join('\n');
        }
        fs.writeFileSync(filePath, content);
    }
    async exportEnv(filePath) {
        const pairs = await this.exportPairs();
        const lines = ['# generated by cascade'];
        for (const [key, value] of Object.entries(pairs)) {
            const name = key.replace(/\./g, '_').toUpperCase();
            const escaped = String(value)
                .replace(/\\/g, '\\\\')
                .replace(/"/g, '\\"')
                .replace(/\n/g, '\\n');
            lines.push(`${name}="${escaped}"`);
        }
        fs.writeFileSync(filePath, lines.join('\n') + '\n');
    }
}
exports.ConfigCenter = ConfigCenter;
//# sourceMappingURL=client.js.map