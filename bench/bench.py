#!/usr/bin/env python3
"""Token-cost benchmark: driving an API as an LLM agent with raw curl vs lazyreq.

Measures the exact text an agent must WRITE (shell commands) and READ
(command output) to complete realistic workflows, against a live local API.
All commands actually run. Token counting uses tiktoken when available.
"""
import base64
import hashlib
import hmac as hmac_mod
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs

PORT = 8971
BASE = f"http://localhost:{PORT}"
SECRET = "bench-webhook-secret"
# realistic JWT-sized token so the "auth tax" is honest
JWT = ("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9."
       "eyJzdWIiOiI0MiIsIm5hbWUiOiJZdXJpIE1pZ3VlbCIsInJvbGVzIjpbImFkbWluIiwiZGV2Il0sImlhdCI6MTc4Mzk4NzYwMSwiZXhwIjoxNzgzOTkxMjAxLCJpc3MiOiJiZW5jaC1hcGkifQ."
       "3fXk9dQ2mVnB8sLwYtR5uHc1KpZaEoJ7NxG4iD6yTqM")
STALE_JWT = JWT[:-6] + "EXPIRD"  # same shape, rejected by the server

USER = {
    "id": 42, "email": "hello@yuri.dev", "name": "Yuri Miguel",
    "roles": ["admin", "dev"], "verified": True,
    "created_at": "2023-01-14T09:22:31Z", "last_login": "2026-07-14T00:06:41Z",
    "preferences": {"theme": "dark", "locale": "pt-BR", "notifications": {"email": True, "push": False}},
    "shop": {"id": 13733, "name": "Loja do Yuri", "plan": "pro", "region": "sa-east-1"},
}


def orders_payload(n):
    return {
        "total": 128, "page": 1, "per_page": n,
        "items": [
            {"id": 12300 + i, "shop_id": 13733, "status": ["paid", "pending", "shipped"][i % 3],
             "amount_cents": 1990 + i * 731, "currency": "BRL",
             "customer": {"id": 900 + i, "email": f"customer{i}@example.com",
                          "name": f"Customer Number {i}", "document": f"123.456.789-{i:02d}"},
             "shipping": {"method": "sedex", "tracking": f"BR{i:09d}XX", "eta_days": 2 + i % 5},
             "created_at": f"2026-07-{10 + (i % 4):02d}T1{i % 10}:0{i % 6}:00Z"}
            for i in range(n)
        ],
    }


class Api(BaseHTTPRequestHandler):
    def _send(self, obj, status=200):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self):
        return self.rfile.read(int(self.headers.get("Content-Length", 0)))

    def do_POST(self):
        if self.path == "/login":
            self._send({"token": JWT, "token_type": "Bearer", "expires_in": 3600,
                        "user": {"id": 42, "email": "hello@yuri.dev"}})
        elif self.path == "/orders":
            self._body()
            self._send({"error": "validation_failed",
                        "details": [{"field": "shop_id", "message": "shop 99999 does not exist"}]}, 422)
        elif self.path == "/webhook":
            self._body()
            self._send({"error": "duplicate_event",
                        "message": "event with this idempotency key was already processed"}, 409)
        elif self.path == "/signed-webhook":
            raw = self._body()
            expected = base64.b64encode(
                hmac_mod.new(SECRET.encode(), raw, hashlib.sha256).digest()
            ).decode()
            if self.headers.get("X-Signature") != expected:
                self._send({"error": "invalid_signature"}, 401)
            else:
                self._send({"ok": True, "signature": "valid"})
        else:
            self._send({"error": "not found"}, 404)

    def do_GET(self):
        if self.headers.get("Authorization") != f"Bearer {JWT}":
            self._send({"error": "unauthorized", "message": "token expired or invalid"}, 401)
            return
        parsed = urlparse(self.path)
        if parsed.path == "/users/me":
            self._send(USER)
        elif parsed.path == "/orders":
            n = int(parse_qs(parsed.query).get("n", ["10"])[0])
            self._send(orders_payload(n))
        else:
            self._send({"error": "not found"}, 404)

    def log_message(self, *a):
        pass


def tokens(text):
    try:
        import tiktoken
        return len(tiktoken.get_encoding("o200k_base").encode(text))
    except Exception:
        return max(1, round(len(text) / 4))


def sh(cmd, env=None):
    r = subprocess.run(cmd, shell=isinstance(cmd, str), capture_output=True, text=True, env=env)
    return (r.stdout + r.stderr)


class Side:
    """Accumulates what the agent writes (commands) and reads (outputs)."""
    def __init__(self):
        self.write = ""
        self.read = ""

    def run(self, cmd, env=None, display_cmd=None):
        out = sh(cmd, env=env)
        self.write += (display_cmd or (cmd if isinstance(cmd, str) else " ".join(cmd))) + "\n"
        self.read += out
        return out

    def note_write(self, text):
        self.write += text

    def cost(self):
        return tokens(self.write), tokens(self.read)


def main():
    server = HTTPServer(("127.0.0.1", PORT), Api)
    threading.Thread(target=server.serve_forever, daemon=True).start()

    lazyreq = os.environ.get("LAZYREQ_BIN") or shutil.which("lazyreq")
    if not lazyreq:
        sys.exit("lazyreq binary not found")

    workdir = tempfile.mkdtemp(prefix="lreq-bench-")
    fake_home = tempfile.mkdtemp(prefix="lreq-bench-home-")  # isolated ~/.lazyreq
    env = {**os.environ, "HOME": fake_home}

    lreq_file = os.path.join(workdir, "api.lreq")
    LREQ = f"""VARS
  baseURL = {BASE}
  webhookSecret = {SECRET}

HOOKS
  login = $req.login 1

ID: login
POST $baseURL/login
H: Content-Type = application/json
{{"email": "hello@yuri.dev", "password": "hunter2"}}

ID: me
GET $baseURL/users/me
H: Authorization = Bearer $login.token

ID: orders
GET $baseURL/orders
H: Authorization = Bearer $login.token

ID: orders-5
GET $baseURL/orders?n=5
H: Authorization = Bearer $login.token

ID: orders-50
GET $baseURL/orders?n=50
H: Authorization = Bearer $login.token

ID: orders-200
GET $baseURL/orders?n=200
H: Authorization = Bearer $login.token

ID: create-order
POST $baseURL/orders
H: Content-Type = application/json
H: Authorization = Bearer $login.token
{{"shop_id": 99999, "amount_cents": 1990}}

ID: webhook
POST $baseURL/webhook
H: Content-Type = application/json
H: Authorization = Bearer $login.token
{{"event": "order.paid", "idempotency_key": "$uuid()", "nonce": "$fuzz_str(16)", "amount_cents": 1990}}

ID: signed
POST $baseURL/signed-webhook
H: Content-Type = application/json
H: X-Signature = $hmac($body, $webhookSecret)
{{"event": "order.paid", "order_id": 12312, "shop_id": 13733, "amount_cents": 1990}}
"""
    with open(lreq_file, "w") as f:
        f.write(LREQ)

    curl_login = f"""curl -s -X POST -H "Content-Type: application/json" -d '{{"email": "hello@yuri.dev", "password": "hunter2"}}' "{BASE}/login\""""
    curl_me = f"""curl -s -H "Authorization: Bearer {JWT}" "{BASE}/users/me\""""
    curl_me_stale = f"""curl -s -H "Authorization: Bearer {STALE_JWT}" "{BASE}/users/me\""""
    curl_orders = f"""curl -s -H "Authorization: Bearer {JWT}" "{BASE}/orders\""""
    curl_create = f"""curl -s -X POST -H "Content-Type: application/json" -H "Authorization: Bearer {JWT}" -d '{{"shop_id": 99999, "amount_cents": 1990}}' "{BASE}/orders\""""

    scenarios = []

    # ---- S0: one-time authoring cost ----------------------------------------
    c, l = Side(), Side()
    l.note_write(LREQ)
    scenarios.append(("S0  author the collection (once)", c, l))

    # ---- S1: cold start — authenticate + GET /users/me ----------------------
    c, l = Side(), Side()
    c.run(curl_login)
    c.run(curl_me)
    l.run(f'{lazyreq} {lreq_file} me', env=env, display_cmd="lazyreq api.lreq me")
    scenarios.append(("S1  cold start: login + GET /users/me", c, l))

    # ---- S2: the same request, 5 more times ----------------------------------
    c, l = Side(), Side()
    for _ in range(5):
        c.run(curl_me)
        l.run(f'{lazyreq} {lreq_file} me', env=env, display_cmd="lazyreq api.lreq me")
    scenarios.append(("S2  repeat GET /users/me ×5", c, l))

    # ---- S3: new session — 'what does /orders return?' -----------------------
    sh(f'{lazyreq} {lreq_file} orders', env=env)  # previous session
    c, l = Side(), Side()
    c.run(curl_login)
    c.run(curl_orders)
    l.run(f'{lazyreq} {lreq_file} orders --history --last 1', env=env,
          display_cmd="lazyreq api.lreq orders --history --last 1")
    scenarios.append(("S3  new session: recall /orders response", c, l))

    # ---- S4: debugging — 'what did I send when it failed?' -------------------
    sh(f'{lazyreq} {lreq_file} create-order', env=env)
    c, l = Side(), Side()
    c.run(curl_create + " -v", display_cmd=curl_create + " -v")
    l.run(f'{lazyreq} {lreq_file} create-order --history --failed -v --last 1', env=env,
          display_cmd="lazyreq api.lreq create-order --history --failed -v --last 1")
    scenarios.append(("S4  debug: inspect the failing POST", c, l))

    # ---- S5: reproduce a failed run EXACTLY (generated payload) --------------
    sh(f'{lazyreq} {lreq_file} webhook', env=env)
    c, l = Side(), Side()
    webhook_body = ('{"event": "order.paid", "idempotency_key": "9c1b2a4e-77d3-4f0a-9d21-3c5e8b6f0a11", '
                    '"nonce": "Xk3pQz7LmN2vR8sT", "amount_cents": 1990}')
    curl_webhook = (f'curl -s -X POST -H "Content-Type: application/json" '
                    f'-H "Authorization: Bearer {JWT}" -d \'{webhook_body}\' "{BASE}/webhook"')
    c.run(curl_webhook + " -v", display_cmd=curl_webhook + " -v")  # earlier failing call, verbose
    c.run(curl_webhook)                                            # reproduction: exact body re-typed
    out = l.run(f'{lazyreq} {lreq_file} --history --status 409 --last 1', env=env,
                display_cmd="lazyreq api.lreq --history --status 409 --last 1")
    run_id = out.split()[0]
    l.run(f'{lazyreq} {lreq_file} --retry {run_id}', env=env,
          display_cmd=f"lazyreq api.lreq --retry {run_id}")
    scenarios.append(("S5  reproduce the failed run exactly", c, l))

    # ---- S6: HMAC-signed webhook ---------------------------------------------
    c, l = Side(), Side()
    signed_body = '{"event": "order.paid", "order_id": 12312, "shop_id": 13733, "amount_cents": 1990}'
    sign_cmd = f"printf '%s' '{signed_body}' | openssl dgst -sha256 -hmac \"{SECRET}\" -binary | base64"
    digest = c.run(sign_cmd).strip()                               # compute signature, read digest
    c.run(f'curl -s -X POST -H "Content-Type: application/json" -H "X-Signature: {digest}" '
          f"-d '{signed_body}' \"{BASE}/signed-webhook\"")         # paste digest into the call
    l.run(f'{lazyreq} {lreq_file} signed', env=env, display_cmd="lazyreq api.lreq signed")
    scenarios.append(("S6  send an HMAC-signed webhook", c, l))

    # ---- S7: expired token mid-session ----------------------------------------
    sh(f'{lazyreq} {lreq_file} me', env=env)   # hook cached with ttl=1...
    time.sleep(1.2)                            # ...and now it's expired
    c, l = Side(), Side()
    c.run(curl_me_stale)                       # 401 with the stale token
    c.run(curl_login)                          # re-authenticate, read new token
    c.run(curl_me)                             # re-paste, retry
    l.run(f'{lazyreq} {lreq_file} me', env=env, display_cmd="lazyreq api.lreq me")
    scenarios.append(("S7  recover from an expired token", c, l))

    # ---- Report -----------------------------------------------------------------
    try:
        import tiktoken  # noqa: F401
        counter = "tiktoken o200k_base"
    except Exception:
        counter = "chars/4 heuristic"

    print(f"\nToken cost per workflow (agent writes + reads), counted with {counter}\n")
    header = f"{'scenario':<44} {'curl':>7} {'lazyreq':>8} {'savings':>8} {'weighted':>9}"
    print(header)
    print("-" * len(header))
    totals = [0, 0, 0, 0]
    for name, c, l in scenarios:
        cw, cr = c.cost()
        lw, lr = l.cost()
        ct, lt = cw + cr, lw + lr
        cwt, lwt = 5 * cw + cr, 5 * lw + lr    # output tokens ≈5× input price
        totals[0] += ct; totals[1] += lt; totals[2] += cwt; totals[3] += lwt
        savings = f"{(1 - lt / ct) * 100:+.0f}%" if ct else "n/a"
        weighted = f"{(1 - lwt / cwt) * 100:+.0f}%" if cwt else "n/a"
        print(f"{name:<44} {ct:>7} {lt:>8} {savings:>8} {weighted:>9}")
    print("-" * len(header))
    print(f"{'TOTAL':<44} {totals[0]:>7} {totals[1]:>8} "
          f"{(1 - totals[1] / totals[0]) * 100:>+7.0f}% {(1 - totals[3] / totals[2]) * 100:>+8.0f}%")

    print("\nwrite = tokens the agent generates (commands); read = output it must ingest\n")
    print(f"{'scenario':<44} {'curl w/r':>12} {'lazyreq w/r':>12}")
    for name, c, l in scenarios:
        cw, cr = c.cost()
        lw, lr = l.cost()
        print(f"{name:<44} {f'{cw}/{cr}':>12} {f'{lw}/{lr}':>12}")

    # ---- Payload-size sweep: recall cost vs response size ------------------------
    print("\nRecall in a new session vs payload size (S3 shape, larger /orders pages)\n")
    print(f"{'payload':<22} {'bytes':>8} {'curl':>7} {'lazyreq':>8} {'savings':>8}")
    for req_id, n in [("orders-5", 5), ("orders-50", 50), ("orders-200", 200)]:
        sh(f'{lazyreq} {lreq_file} {req_id}', env=env)  # previous session
        size = len(json.dumps(orders_payload(n)))
        c, l = Side(), Side()
        c.run(curl_login)
        c.run(f'curl -s -H "Authorization: Bearer {JWT}" "{BASE}/orders?n={n}"')
        l.run(f'{lazyreq} {lreq_file} {req_id} --history --last 1', env=env,
              display_cmd=f"lazyreq api.lreq {req_id} --history --last 1")
        ct, lt = sum(c.cost()), sum(l.cost())
        print(f"{f'/orders ({n} items)':<22} {size:>8} {ct:>7} {lt:>8} {(1 - lt / ct) * 100:>+7.0f}%")

    server.shutdown()


if __name__ == "__main__":
    main()
