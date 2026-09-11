"""Cascade SDK — HTTP + SSE client with snapshot fallback.

Usage:
    cc = ConfigCenter.from_url("cascade://localhost:7070/project/<id>?env=prod&token=xxx")
    cc.get("database.host")
    cc.watch(lambda event: ...); cc.start_watch()
    cc.export_file("./config.yaml")
    cc.export_env("./.env")

Secrets are masked (None) unless the client was created with reveal=True AND
the token has admin permission. The snapshot file stores exactly what the
server returned — keep reveal off if the snapshot location is not trusted.
"""

import json
import os
import threading
import time
from typing import Any, Callable, Dict, List, Optional

try:
    import httpx
    HAS_HTTPX = True
except ImportError:
    HAS_HTTPX = False


def _need_httpx():
    if not HAS_HTTPX:
        raise ImportError("httpx is required: pip install httpx")


class ConfigCenter:
    def __init__(
        self,
        server: Optional[str] = None,
        project_id: Optional[str] = None,
        token: Optional[str] = None,
        env: Optional[str] = None,
        reveal: bool = False,
        snapshot_path: Optional[str] = None,
    ):
        self.server = (server or "http://localhost:7070").rstrip("/")
        self.project_id = project_id
        self.token = token
        self.env = env
        self.reveal = reveal
        self.snapshot_path = snapshot_path or os.path.join(
            os.path.expanduser("~"), ".cascade", "sdk", f"{project_id}.json"
        )
        self._callbacks: Dict[str, List[Callable]] = {"change": []}
        self._cache: Optional[List[dict]] = None
        self._cache_ts = 0.0
        self._watch_started = False

    # ── construction ────────────────────────────────────────────
    @classmethod
    def from_url(cls, url: str) -> "ConfigCenter":
        """Create a client from a project link.

        Canonical:  cascade://<host>[:port]/project/<id>?env=<name>&token=<t>[&reveal=1]
        Shorthand:  cascade://project/<id>   (server defaults to http://localhost:7070)

        Legacy `cc://` links parse identically (scheme is ignored).
        """
        from urllib.parse import urlparse, parse_qs

        parsed = urlparse(url)
        q = parse_qs(parsed.query)
        token = q.get("token", [None])[0]
        env = q.get("env", [None])[0]
        reveal = q.get("reveal", ["false"])[0].lower() in ("1", "true", "yes")

        if parsed.netloc in ("", "project"):
            server = "http://localhost:7070"
            project_id = parsed.path.strip("/").split("/")[-1]
        else:
            host = parsed.hostname or "localhost"
            port = parsed.port or 7070
            server = f"http://{host}:{port}"
            project_id = parsed.path.replace("/project/", "").strip("/")
        if not project_id:
            raise ValueError(f"cannot parse project id from {url!r}")
        return cls(server=server, project_id=project_id, token=token, env=env, reveal=reveal)

    # ── internals ───────────────────────────────────────────────
    def _headers(self) -> Dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        return headers

    def _params(self) -> Dict[str, str]:
        p: Dict[str, str] = {}
        if self.env:
            p["env"] = self.env
        if self.reveal:
            p["reveal"] = "true"
        return p

    def _fetch(self) -> List[dict]:
        _need_httpx()
        resp = httpx.get(
            f"{self.server}/api/projects/{self.project_id}/resolved",
            headers=self._headers(),
            params=self._params(),
        )
        resp.raise_for_status()
        data = resp.json()
        self._write_snapshot(data)
        self._cache, self._cache_ts = data, time.time()
        return data

    def _write_snapshot(self, data: List[dict]) -> None:
        try:
            parent = os.path.dirname(self.snapshot_path)
            if parent:
                os.makedirs(parent, exist_ok=True)
            with open(self.snapshot_path, "w") as f:
                json.dump(data, f)
        except OSError:
            pass

    def _read_snapshot(self) -> List[dict]:
        try:
            with open(self.snapshot_path) as f:
                return json.load(f)
        except (OSError, ValueError):
            return []

    def _data(self, max_age: float = 5.0, force: bool = False) -> List[dict]:
        """Resolved configs, cached briefly. Falls back to the last snapshot
        when the server is unreachable (startup must not be blocked)."""
        if not force and self._cache is not None and time.time() - self._cache_ts < max_age:
            return self._cache
        try:
            return self._fetch()
        except Exception:
            snap = self._read_snapshot()
            self._cache, self._cache_ts = snap, time.time()
            return snap

    # ── reads ───────────────────────────────────────────────────
    def get(self, key: str, default: Any = None) -> Any:
        """Effective value of a key for this project/env.
        Hidden secrets (no reveal permission) return `default`."""
        for c in self._data():
            if c["key"] == key:
                return c["value"] if c["value"] is not None else default
        return default

    def list_configs(self, group: Optional[str] = None) -> List[dict]:
        return [c for c in self._data() if not group or c.get("group") == group]

    def refresh(self) -> List[dict]:
        """Force a fetch (or snapshot fallback) and return resolved configs."""
        return self._data(force=True)

    # ── writes (global config entries) ──────────────────────────
    def set(self, key: str, value: str, secret: bool = False) -> dict:
        _need_httpx()
        resp = httpx.post(
            f"{self.server}/api/configs",
            headers=self._headers(),
            json={"key": key, "value": value, "secret": secret},
        )
        resp.raise_for_status()
        self._cache = None
        return resp.json()

    # ── realtime (SSE) ──────────────────────────────────────────
    def watch(self, callback: Callable[[str], None]) -> None:
        """Register a change callback; it receives the event string,
        e.g. "config_updated:<id>". Call start_watch() to begin streaming."""
        self._callbacks["change"].append(callback)

    def start_watch(self) -> None:
        """Start the SSE listener in a daemon thread with auto-reconnect."""
        if self._watch_started:
            return
        self._watch_started = True
        threading.Thread(target=self._watch_loop, daemon=True).start()

    def _watch_loop(self) -> None:
        while True:
            try:
                with httpx.stream(
                    "GET",
                    f"{self.server}/api/sse/configs",
                    headers=self._headers(),
                    timeout=None,
                ) as r:
                    for line in r.iter_lines():
                        if not line.startswith("data:"):
                            continue
                        event = line[len("data:"):].strip()
                        self._cache = None  # next read refetches
                        for cb in list(self._callbacks["change"]):
                            try:
                                cb(event)
                            except Exception:
                                pass
            except Exception:
                pass
            time.sleep(3)  # reconnect backoff

    # ── exports ─────────────────────────────────────────────────
    def _export_pairs(self) -> Dict[str, str]:
        out: Dict[str, str] = {}
        for c in self._data(force=True):
            key = c["key"]
            if c["value"] is None:
                out[key] = "${" + key.replace(".", "_").upper() + "}"
            else:
                out[key] = c["value"]
        return out

    def export_file(self, path: str, format: str = "yaml") -> None:
        data = self._export_pairs()
        if format == "json":
            content = json.dumps(data, indent=2)
        elif format == "yaml":
            try:
                import yaml
            except ImportError as e:
                raise ImportError("pyyaml is required for yaml export: pip install pyyaml") from e
            content = yaml.safe_dump(data, sort_keys=False, allow_unicode=True)
        else:
            raise ValueError(f"unsupported format: {format}")
        with open(path, "w") as f:
            f.write(content)

    def export_env(self, path: str) -> None:
        lines = ["# generated by cascade"]
        for key, value in self._export_pairs().items():
            name = key.replace(".", "_").upper()
            escaped = value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
            lines.append(f'{name}="{escaped}"')
        with open(path, "w") as f:
            f.write("\n".join(lines) + "\n")
