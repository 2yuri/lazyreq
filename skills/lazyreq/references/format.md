# The .lreq format — full reference

A `.lreq` file has three kinds of sections: `VARS`, `HOOKS`, and one block per request starting with `ID:`. Blank lines are ignored; `#` at the start of a line is a comment.

## VARS

```lreq
VARS
  baseURL = http://localhost:8080
  apiKey = $env.MY_API_KEY
```

- `name = value` pairs. Values may be `$env.VAR_NAME` (resolved at parse time; missing env var = error).
- One pair of surrounding `"` or `'` quotes is stripped; inner quotes survive.

## HOOKS

```lreq
HOOKS
  login = $req.login 300
  me = $req.me
```

- `name = $req.<request-id> [ttl-seconds]`. Using `$name.field` anywhere executes that request and drills into its JSON response (`$login.token`, `$login.user.id`).
- With a TTL, the response is cached (encrypted) under `~/.lazyreq/cache/`, keyed by the file's absolute path + request id. Without a TTL the hook runs on every use.
- A hook whose request returns non-2xx is an error.
- Hooks may reference other hooks; recursion depth is capped at 16.

## Requests

```lreq
ID: create-user
DESCRIPTION: Create a user with a signed payload
POST $baseURL/users
H: Content-Type = application/json
H: X-Request-Id = $uuid()
H: X-Signature = $hmac($body, $env.SECRET)
M: avatar = file://./photo.png
{"name": "$fuzz_str(8)"}
```

| Line | Meaning |
|---|---|
| `ID: name` | starts a request block; ids must be unique in the file |
| `DESCRIPTION: text` | optional; shown by `--list` |
| `METHOD url` | `GET`/`POST`/`PUT`/`DELETE`/`PATCH`/`HEAD`/`OPTIONS`; must come before headers refer to the body |
| `H: Name = value` | header |
| `M: name = value` | multipart form field (switches the request to `multipart/form-data`) |
| anything else | accumulated into the request body |

Multipart value prefixes: `file://path` uploads a local file; `download://url` fetches the URL at runtime and uploads the bytes. With multipart, any explicit `Content-Type` header is dropped (the boundary is set automatically).

`key = value` lines split on the **first** `=` only, so values containing `=` (base64, signatures) work. Keys must be a single bare word (`[A-Za-z0-9_-]+`).

## Interpolation

| Token | Resolves to | Where |
|---|---|---|
| `$name` | variable from `VARS` | everywhere |
| `$env.NAME` | environment variable | everywhere |
| `$hook.path.to.field` | runs the hook's request, drills into its JSON | everywhere |
| `$body` | the final interpolated body of the current request | URLs, headers, multipart values only |
| `$$` | a literal `$` | everywhere |

- **Strict contexts** (URL, header values, multipart values): unknown `$tokens` are errors.
- **Lenient context** (bodies): unknown `$tokens` pass through untouched, so MongoDB-style `{"$gte": 5}` works.
- Resolution order per request: body first (lenient), then URL/headers/multipart (strict, with `$body` available). This is what makes body signing correct.

## Built-in functions

| Function | Result |
|---|---|
| `$uuid()` | random UUID v4 |
| `$fuzz_str(len)` | random alphanumeric string of `len` chars |
| `$fuzz_int(min, max)` | random integer, inclusive |
| `$hmac(data, key, algo?, encoding?)` | HMAC; `algo`: `sha1`/`sha256`/`sha512` (default `sha256`); `encoding`: `hex`/`base64` (default `base64`) |

Each occurrence evaluates independently (two `$uuid()` = two UUIDs). Arguments may themselves be `$vars`, `$env.X` or `$body`.

## History & storage

- Every executed request — including hook-triggered ones — appends an encrypted record (timestamp, id, method, resolved URL, request headers/body, status, latency, response body) under `~/.lazyreq/history/`, keyed by the file's absolute path. Transport failures are recorded too (status `ERR` + the error message).
- The newest 20 runs per request id are kept.
- `--curl` and `--list` execute nothing and record nothing.
- Everything under `~/.lazyreq/` (cache + history) is gzipped then encrypted with XChaCha20-Poly1305. The key is auto-generated at `~/.lazyreq/key` (0600); setting `LAZYREQ_KEY` overrides it. Files written under one key are unreadable (treated as empty) under another.

## Errors

Parse errors carry the file, line number and a hint; runtime errors name the request and hook chain. Exit code is 1 on any error.
