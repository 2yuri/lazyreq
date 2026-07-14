---
name: lazyreq
description: Author .lreq files and run HTTP requests with the lazyreq CLI. Use when working with .lreq files, testing HTTP APIs via lazyreq, converting curl commands to request collections, or checking past request runs/results via lazyreq history.
---

# lazyreq

lazyreq is a file-based HTTP client. Requests live in `.lreq` files (variables, auth hooks, signing functions) and run by name. It records every run into an **encrypted local history** you can query — so check history before re-running requests, instead of re-executing them just to see what they return.

## Commands

```sh
lazyreq <file.lreq> --list                # request ids + methods + urls (start here)
lazyreq <file.lreq> <id>                  # run a request, print response
lazyreq <file.lreq> <id> --curl           # print as a curl command (does not run)
lazyreq import '<curl command>'           # convert curl → .lreq block (append with >>)

# history — past runs, compact by default; first column is the run id
lazyreq <file.lreq> --history             # one line per run (run id, status, latency, size)
lazyreq <file.lreq> <id> --history        # runs of one request + response JSON shape
lazyreq <file.lreq> <id> --history -v     # + resolved URL, request body, full response
lazyreq <file.lreq> --history --req <run-id> -v  # one specific run, fully detailed
lazyreq <file.lreq> --history --failed    # filters: --success | --failed | --status 401
lazyreq <file.lreq> --history --last 3    # limit to the N most recent

# retry — replay a recorded run by its run id
lazyreq <file.lreq> --retry <run-id>      # exact recorded URL+body, fresh headers/auth
```

## Workflow

1. **Orient**: `--list` shows what exists; read the `.lreq` file for details.
2. **Check history first**: `lazyreq api.lreq me --history --last 3` tells you whether a request recently succeeded and what shape its response has — often that answers the question with ~30 tokens, without re-running anything. Never re-run a POST/PUT/DELETE just to recall its response.
3. **Run** what you need. Every run (including hook-triggered logins) is recorded automatically.
4. **Debug failures** with `--history --failed -v`: you get the resolved URL and body that were actually sent, plus each run's id. Add `--show-headers` only when debugging headers specifically — it prints live auth tokens into your context.
5. **Reproduce exactly** with `--retry <run-id>`: replays the recorded body byte-for-byte (generated `$uuid()`/`$fuzz_*()` values included — a normal re-run would regenerate them) while re-resolving headers so auth is fresh and `$hmac($body, ...)` is recomputed. Never hand-rebuild a curl command from `-v` output to reproduce a request. Retrying re-executes the request — for POST/PUT/DELETE that mutates state, so retry deliberately, not to recall information.

## Authoring .lreq files

Minimal anatomy (full spec: [references/format.md](references/format.md)):

```lreq
VARS
  baseURL = http://localhost:8080
  apiKey = $env.MY_API_KEY        # secrets come from the environment

HOOKS
  login = $req.login 300           # run `login`, cache its JSON response 300s

ID: login
POST $baseURL/auth/login
H: Content-Type = application/json
{"email": "x@y.z", "password": "$env.PASSWORD"}

ID: me
DESCRIPTION: Current user
GET $baseURL/users/me
H: Authorization = Bearer $login.token   # drills into the hook's JSON response
```

Rules that matter when generating files:

- `METHOD url` must be the first line after `ID:`/`DESCRIPTION:`. Headers are `H: Name = value`, multipart fields `M: name = value`, everything else becomes the body.
- `$token` interpolation is **strict** in URLs/headers/multipart (unknown tokens are errors) but **lenient** in bodies (`{"$gte": 5}` passes through). Escape a literal dollar as `$$`.
- `key = value` splits on the first `=` only; one pair of surrounding quotes is stripped.
- Functions: `$uuid()`, `$fuzz_str(len)`, `$fuzz_int(min, max)`, `$hmac(data, key, algo?, encoding?)`. `$body` (URLs/headers only) is the final interpolated body — `H: X-Signature = $hmac($body, $env.SECRET)` signs exactly what is sent.
- Never hardcode secrets in the file; use `$env.X`.

Judgment calls:

- **Auth chain** → a `login` request + a hook with a TTL slightly shorter than the token's lifetime; consumers write `$login.token`.
- **Converting docs or devtools output** → prefer `lazyreq import 'curl ...'` over hand-writing, then replace literal hosts/tokens with `$vars` and `$env.X`.
- **After editing a file** → `lazyreq file.lreq --list` is a cheap parse check; errors carry line numbers and hints.

## Hard rules

- Never read or write files under `~/.lazyreq/` directly — they are encrypted (gzip + XChaCha20-Poly1305; key in `~/.lazyreq/key`, overridable via `LAZYREQ_KEY`). The only interface is the CLI.
- Never re-run mutating requests to recover information that `--history` already has.
- `--curl` still resolves hooks, so it can hit the auth endpoint; it does not run the request itself and records no history for it.
