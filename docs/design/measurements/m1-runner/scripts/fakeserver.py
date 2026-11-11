#!/usr/bin/env python3
"""Tiny fake GitHub/broker endpoint for the offline runner measurements. Binds 127.0.0.1 only.

usage: fakeserver.py PORTFILE LOGFILE MODE
  PORTFILE  written with the chosen port once listening
  LOGFILE   one JSON object per request: method, path, query, headers (secrets redacted), body (redacted, truncated)
  MODE      ok         token 200, POST /session 200, GET /message long-polls 3 s then 202, DELETE /session 204
            session404 token 200, POST /session 404
            session403 token 200, POST /session 403 (plain text body)
            tooold     like ok, but GET /message answers the broker error RunnerVersionTooOld
            token401   POST /oauth/token answers 401 invalid_client
Everything else answers 404 and is logged.
"""
import json, sys, time, uuid, re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit, parse_qsl

portfile, logfile, mode = sys.argv[1], sys.argv[2], sys.argv[3]
SESSION_ID = str(uuid.uuid4())
SECRET_HEADERS = {"authorization", "cookie", "proxy-authorization"}


def redact_body(raw: bytes, ctype: str) -> str:
    text = raw.decode("utf-8", "replace")
    if "x-www-form-urlencoded" in ctype:
        pairs = parse_qsl(text, keep_blank_values=True)
        out = []
        for k, v in pairs:
            if k in ("client_assertion", "assertion", "refresh_token", "client_secret", "code"):
                v = "<redacted len=%d>" % len(v)
            out.append("%s=%s" % (k, v))
        return "&".join(out)
    return text[:400]


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def _record(self, status):
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else b""
        u = urlsplit(self.path)
        hdrs = {}
        for k, v in self.headers.items():
            if k.lower() in SECRET_HEADERS:
                v = v.split(" ")[0] + " <redacted len=%d>" % len(v)
            hdrs[k] = v
        rec = {"t": round(time.time(), 3), "method": self.command, "path": u.path,
               "query": dict(parse_qsl(u.query)), "headers": hdrs,
               "body": redact_body(body, self.headers.get("Content-Type", "")), "status": status}
        with open(logfile, "a") as f:
            f.write(json.dumps(rec) + "\n")
        return body

    def _send(self, status, obj=None, raw=None, ctype="application/json"):
        data = raw if raw is not None else (json.dumps(obj).encode() if obj is not None else b"")
        self.send_response(status)
        if data:
            self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if data:
            self.wfile.write(data)

    def handle_any(self):
        path = urlsplit(self.path).path
        m = self.command
        if m == "POST" and path.endswith("/oauth/token"):
            if mode == "token401":
                self._record(401)
                return self._send(401, {"error": "invalid_client"})
            self._record(200)
            return self._send(200, {"access_token": "dummy-access-token", "token_type": "bearer", "expires_in": 3600})
        if m == "POST" and path.endswith("/session"):
            body = self._record({"session404": 404, "session403": 403}.get(mode, 200))
            if mode == "session404":
                return self._send(404, raw=b"not found", ctype="text/plain")
            if mode == "session403":
                return self._send(403, raw=b"forbidden", ctype="text/plain")
            req = json.loads(body or b"{}")
            return self._send(200, {"sessionId": SESSION_ID, "ownerName": req.get("ownerName", "x"),
                                    "agent": req.get("agent"), "brokerSession": True})
        if m == "GET" and path.endswith("/message"):
            if mode == "tooold":
                self._record(400)
                return self._send(400, {"source": "actions-broker-listener", "errorKind": "RunnerVersionTooOld",
                                        "statusCode": 400, "errorMessage": "synthetic: runner version too old"})
            self._record(202)
            time.sleep(3)
            return self._send(202)
        if m == "DELETE" and path.endswith("/session"):
            self._record(204)
            return self._send(204)
        self._record(404)
        return self._send(404, raw=b"not found", ctype="text/plain")

    do_GET = do_POST = do_DELETE = do_PUT = do_OPTIONS = do_HEAD = handle_any


srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
srv.daemon_threads = True
open(portfile, "w").write(str(srv.server_address[1]))
srv.serve_forever()
