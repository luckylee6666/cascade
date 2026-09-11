import { Config, ChangeCallback } from './models';

const fs = require('fs');
const path = require('path');
const os = require('os');

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
export class ConfigCenter {
  private server: string;
  private projectId: string;
  private token: string;
  private env: string | null;
  private reveal: boolean;
  private snapshotPath: string;
  private callbacks: ChangeCallback[] = [];
  private cache: ResolvedConfig[] | null = null;
  private cacheTs = 0;
  private watching = false;

  constructor(
    server: string,
    projectId: string,
    token: string = '',
    env: string | null = null,
    reveal: boolean = false,
    snapshotPath: string | null = null,
  ) {
    this.server = server.replace(/\/+$/, '');
    this.projectId = projectId;
    this.token = token;
    this.env = env;
    this.reveal = reveal;
    this.snapshotPath =
      snapshotPath || path.join(os.homedir(), '.cascade', 'sdk', `${projectId}.json`);
  }

  /**
   * Canonical:  cascade://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]
   * Shorthand:  cascade://project/<id>   (server defaults to http://localhost:7070)
   * Legacy `cc://` links parse identically (scheme is ignored).
   */
  static fromUrl(url: string): ConfigCenter {
    const parsed = new URL(url);
    const token = parsed.searchParams.get('token') || '';
    const env = parsed.searchParams.get('env');
    const reveal = ['1', 'true', 'yes'].includes(
      (parsed.searchParams.get('reveal') || '').toLowerCase(),
    );
    let server: string;
    let projectId: string;
    if (parsed.host === 'project' || parsed.host === '') {
      server = 'http://localhost:7070';
      const segs = parsed.pathname.split('/').filter(Boolean);
      projectId = segs[segs.length - 1] || '';
    } else {
      server = `http://${parsed.hostname}:${parsed.port || '7070'}`;
      projectId = parsed.pathname.replace('/project/', '').replace(/^\/+|\/+$/g, '');
    }
    if (!projectId) {
      throw new Error(`cannot parse project id from ${url}`);
    }
    return new ConfigCenter(server, projectId, token, env, reveal);
  }

  // ── internals ───────────────────────────────────────────────
  private headers(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }
    return headers;
  }

  private resolvedUrl(): string {
    const u = new URL(`${this.server}/api/projects/${this.projectId}/resolved`);
    if (this.env) u.searchParams.set('env', this.env);
    if (this.reveal) u.searchParams.set('reveal', 'true');
    return u.toString();
  }

  private async fetchResolved(): Promise<ResolvedConfig[]> {
    const response = await fetch(this.resolvedUrl(), { headers: this.headers() });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = (await response.json()) as ResolvedConfig[];
    this.writeSnapshot(data);
    this.cache = data;
    this.cacheTs = Date.now();
    return data;
  }

  private writeSnapshot(data: ResolvedConfig[]): void {
    try {
      fs.mkdirSync(path.dirname(this.snapshotPath), { recursive: true });
      fs.writeFileSync(this.snapshotPath, JSON.stringify(data));
    } catch (e) {
      /* snapshot is best-effort */
    }
  }

  private readSnapshot(): ResolvedConfig[] {
    try {
      return JSON.parse(fs.readFileSync(this.snapshotPath, 'utf8'));
    } catch (e) {
      return [];
    }
  }

  /** Cached briefly; falls back to the last snapshot when the server is down. */
  private async data(maxAgeMs: number = 5000): Promise<ResolvedConfig[]> {
    if (this.cache && Date.now() - this.cacheTs < maxAgeMs) {
      return this.cache;
    }
    try {
      return await this.fetchResolved();
    } catch (e) {
      const snap = this.readSnapshot();
      this.cache = snap;
      this.cacheTs = Date.now();
      return snap;
    }
  }

  // ── reads ───────────────────────────────────────────────────
  async get(key: string, fallback: string | null = null): Promise<string | null> {
    for (const c of await this.data()) {
      if (c.key === key) {
        return c.value !== null ? c.value : fallback;
      }
    }
    return fallback;
  }

  async listConfigs(group?: string): Promise<Config[]> {
    const all = await this.data();
    return group ? all.filter(c => c.group === group) : all;
  }

  /** Force a fetch (or snapshot fallback) and return resolved configs. */
  async refresh(): Promise<ResolvedConfig[]> {
    try {
      return await this.fetchResolved();
    } catch (e) {
      const snap = this.readSnapshot();
      this.cache = snap;
      this.cacheTs = Date.now();
      return snap;
    }
  }

  // ── writes ──────────────────────────────────────────────────
  async set(key: string, value: string, secret: boolean = false): Promise<Config> {
    const response = await fetch(`${this.server}/api/configs`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ key, value, secret }),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    this.cache = null;
    return (await response.json()) as Config;
  }

  async delete(id: string): Promise<void> {
    const response = await fetch(`${this.server}/api/configs/${id}`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    this.cache = null;
  }

  // ── realtime (SSE) ──────────────────────────────────────────
  on(event: 'change', callback: ChangeCallback): void {
    if (event === 'change') this.callbacks.push(callback);
  }

  /** Start the SSE listener with auto-reconnect (Node 18+ fetch). */
  startWatch(): void {
    if (this.watching) return;
    this.watching = true;
    void this.watchLoop();
  }

  private async watchLoop(): Promise<void> {
    for (;;) {
      try {
        const res = await fetch(`${this.server}/api/sse/configs`, {
          headers: this.headers(),
        });
        if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buf = '';
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });
          const lines = buf.split('\n');
          buf = lines.pop() || '';
          for (const line of lines) {
            if (!line.startsWith('data:')) continue;
            const event = line.slice(5).trim();
            this.cache = null;
            for (const cb of [...this.callbacks]) {
              try {
                cb(event);
              } catch (e) {
                /* keep streaming */
              }
            }
          }
        }
      } catch (e) {
        /* reconnect below */
      }
      await new Promise(r => setTimeout(r, 3000));
    }
  }

  // ── exports ─────────────────────────────────────────────────
  private async exportPairs(): Promise<Record<string, string>> {
    const out: Record<string, string> = {};
    for (const c of await this.refresh()) {
      out[c.key] =
        c.value !== null
          ? c.value
          : '${' + c.key.replace(/\./g, '_').toUpperCase() + '}';
    }
    return out;
  }

  async exportFile(filePath: string, format: 'yaml' | 'json' = 'yaml'): Promise<void> {
    const data = await this.exportPairs();
    let content: string;
    if (format === 'json') {
      content = JSON.stringify(data, null, 2);
    } else {
      content = Object.entries(data)
        .map(([k, v]) => `${k}: "${String(v).replace(/"/g, '\\"')}"`)
        .join('\n');
    }
    fs.writeFileSync(filePath, content);
  }

  async exportEnv(filePath: string): Promise<void> {
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
