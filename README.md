<p align="center">
  <img src="logo.png" alt="lazyreq" width="140" />
</p>

# lazyreq

A lazy, file-based HTTP client for your terminal. Describe your requests once in a plain-text `.lreq` file — with variables, auth hooks, and built-in functions — and run them by name.

```sh
lazyreq api.lreq login
```

No GUI, no workspace sync, no JSON exports. Your API collection is a text file that lives in your repo and diffs like code.

## Install

**From a release** — grab a binary for macOS (universal), Linux (x86_64/arm64) or Windows from the [releases page](https://github.com/2yuri/lazyreq/releases) and put it on your `PATH`.

**From source:**

```sh
cargo install --git https://github.com/2yuri/lazyreq
```

There is also a [VS Code / Cursor extension](https://github.com/2yuri/lazyreq-vscode) that adds syntax highlighting and ▶ Run / Copy-as-curl buttons to `.lreq` files.

## Usage

```sh
lazyreq <file.lreq> <request-id>          # run a request
lazyreq <file.lreq> <request-id> --curl   # print it as a curl command instead
lazyreq <file.lreq> --list                # list every request in the file
```

## The .lreq format

A file has three kinds of sections: `VARS`, `HOOKS`, and one block per request starting with `ID:`.

```lreq
VARS
  baseURL = http://localhost:8080
  path = api/v1
  # values can come from the environment:
  apiKey = $env.MY_API_KEY

HOOKS
  # `login` runs the request with ID `login` and caches its
  # response for 30 seconds (omit the number to never cache)
  login = $req.login 30

ID: login
POST $baseURL/$path/login
H: Content-Type = application/json
{
  "email": "hello@yuri.dev",
  "password": "hunter2"
}

ID: me
GET $baseURL/$path/users/me
H: Authorization = Bearer $login.token
```

Running `lazyreq api.lreq me` executes `login` first (or reuses the cached response), extracts `.token` from its JSON, and sends the authenticated request.

### Line types inside a request

| Line | Meaning |
|---|---|
| `METHOD url` | `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD` or `OPTIONS` — must come first |
| `H: Name = value` | a header |
| `M: name = value` | a multipart form field (sets up `multipart/form-data`) |
| anything else | accumulated into the request body |
| `# ...` | comment |

Multipart values have two special prefixes:

```lreq
M: image = file://./photo.png                 # upload a local file
M: image = download://https://example.com/a.png   # fetch a URL, upload the bytes
```

`key = value` lines split on the **first** `=`, so values containing `=` (tokens, base64 signatures) just work. A value wrapped in one pair of `"` or `'` has the quotes stripped.

### Interpolation

Every `$token` below works in URLs, header values and multipart values. Bodies are interpolated too, but **leniently**: an unknown `$word` in a body is left untouched, so JSON like `{"$gte": 5}` keeps working. Use `$$` anywhere for a literal `$`.

| Token | Resolves to |
|---|---|
| `$name` | a variable from `VARS` |
| `$env.NAME` | an environment variable |
| `$hook.path.to.field` | runs the hook's request, drills into its JSON response |
| `$body` | the final body of the *current* request (URLs/headers/multipart only) |

### Built-in functions

| Function | Result |
|---|---|
| `$uuid()` | a random UUID v4 |
| `$fuzz_str(len)` | a random alphanumeric string |
| `$fuzz_int(min, max)` | a random integer, inclusive |
| `$hmac(data, key, algo?, encoding?)` | HMAC of `data` — `algo`: `sha1`/`sha256`/`sha512` (default `sha256`), `encoding`: `hex`/`base64` (default `base64`) |

Each occurrence is evaluated independently — two `$uuid()` in one body produce two different UUIDs. Function arguments can themselves be variables, `$env.X`, or `$body`.

Signing a webhook body:

```lreq
ID: webhook
POST $baseURL/webhook
H: Content-Type = application/json
H: X-Signature = $hmac($body, $env.WEBHOOK_SECRET)
{
  "event": "user.updated",
  "id": "$uuid()"
}
```

The body is resolved first (fuzz values and all), then `$hmac($body, ...)` signs exactly the bytes that get sent.

### Hooks and caching

```lreq
HOOKS
  login = $req.login 30
```

`$req.<id>` names the request to run; the optional number is a cache TTL in seconds. Cached responses live in `~/.lazyreq/cache/` keyed by (file, request id), so a login token is fetched once and reused until it expires. Without a TTL the hook runs on every use.

### Exporting curl

```sh
$ lazyreq api.lreq me --curl
curl -X GET \
  -H "Authorization: Bearer eyJhbG..." \
  "http://localhost:8080/api/v1/users/me"
```

Hooks are resolved during export, so the command is ready to paste. (That also means exporting can hit your auth endpoint — cached per the hook's TTL.)

### Errors

Failures are reported with context, not stack traces:

```
error: invalid line 14 of api.lreq:
  H: X-Loop-Signature "abc..."
  hint: headers use `H: Name = value`

error: request `me` failed:
  field `token` not found in response of hook `login` (available fields: error)
```

The exit code is 1 on any error, so `.lreq` files behave in scripts.

## Development

```sh
cargo build
cargo run -- example.lreq --list
```

Releases are cut by pushing a `v*` tag — GitHub Actions cross-compiles all targets with GoReleaser + cargo-zigbuild. PRs get a snapshot build as a smoke test.
