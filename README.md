<p align="center">
  <img src="logo.png" alt="lazyreq" width="140" />
</p>

<h1 align="center">lazyreq</h1>

<p align="center">
  A lazy, file-based HTTP client for your terminal.
  <br />
  <a href="https://github.com/2yuri/lazyreq/releases"><img src="https://img.shields.io/github/v/release/2yuri/lazyreq?include_prereleases" alt="release" /></a>
  <a href="https://github.com/2yuri/lazyreq/actions/workflows/release.yml"><img src="https://github.com/2yuri/lazyreq/actions/workflows/release.yml/badge.svg" alt="build" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license" /></a>
</p>

Describe your requests once in a plain-text `.lreq` file — with variables, auth hooks, and built-in functions — and run them by name:

```sh
lazyreq api.lreq login
```

**Why lazyreq?**

- **Your API collection is a text file.** It lives in your repo, diffs like code, and works in CI. No GUI, no cloud workspace, no JSON exports.
- **Auth that gets out of the way.** Hooks run your login request automatically, extract the token from its JSON, and cache it — every other request just says `$login.token`.
- **Sign, fuzz, generate.** Built-in `$hmac()`, `$uuid()`, `$fuzz_str()` and `$fuzz_int()` cover webhook signatures and quick fuzzing without leaving the file.
- **Escape hatch included.** Any request exports as a ready-to-paste `curl` command.

## Quick start

**1.** Install — grab a binary for macOS (universal), Linux (x86_64/arm64) or Windows from the [releases page](https://github.com/2yuri/lazyreq/releases) and put it on your `PATH`, or build with cargo:

```sh
cargo install --git https://github.com/2yuri/lazyreq
```

**2.** Create `api.lreq`:

```lreq
VARS
  baseURL = http://localhost:8080

HOOKS
  login = $req.login 30

ID: login
POST $baseURL/login
H: Content-Type = application/json
{
  "email": "hello@yuri.dev",
  "password": "hunter2"
}

ID: me
GET $baseURL/users/me
H: Authorization = Bearer $login.token
```

**3.** Run it:

```sh
$ lazyreq api.lreq me
[GET] http://localhost:8080/users/me
Status: 200 OK
{
  "id": 42,
  "email": "hello@yuri.dev"
}
```

`login` ran first (and was cached for 30 seconds), its `token` field became the `Authorization` header, and you never typed a token.

Prefer clicking? There's a [VS Code / Cursor extension](https://github.com/2yuri/lazyreq-vscode) with syntax highlighting and ▶ Run / Copy-as-curl buttons.

## CLI

```sh
lazyreq <file.lreq> <request-id>          # run a request
lazyreq <file.lreq> <request-id> --curl   # print it as a curl command instead
lazyreq <file.lreq> --list                # list every request in the file
```

```sh
$ lazyreq api.lreq --list
login  POST    $baseURL/login
me     GET     $baseURL/users/me
```

## The .lreq format

A file has three kinds of sections: `VARS`, `HOOKS`, and one block per request starting with `ID:`.

### VARS

```lreq
VARS
  baseURL = http://localhost:8080
  # values can come from the environment:
  apiKey = $env.MY_API_KEY
```

### HOOKS

```lreq
HOOKS
  login = $req.login 30
```

`$req.<id>` names the request to run; the optional number is a cache TTL in seconds. Cached responses live in `~/.lazyreq/cache/` keyed by (file, request id), so a login token is fetched once and reused until it expires. Without a TTL the hook runs on every use.

### Requests

| Line | Meaning |
|---|---|
| `ID: name` | starts a request block |
| `METHOD url` | `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD` or `OPTIONS` — must come first |
| `H: Name = value` | a header |
| `M: name = value` | a multipart form field (sets up `multipart/form-data`) |
| anything else | accumulated into the request body |
| `# ...` | comment |

Multipart values have two special prefixes:

```lreq
M: image = file://./photo.png                       # upload a local file
M: image = download://https://example.com/a.png     # fetch a URL, upload the bytes
```

`key = value` lines split on the **first** `=`, so values containing `=` (tokens, base64 signatures) just work. A value wrapped in one pair of `"` or `'` has the quotes stripped.

## Interpolation

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

## Exporting curl

```sh
$ lazyreq api.lreq me --curl
curl -X GET \
  -H "Authorization: Bearer eyJhbG..." \
  "http://localhost:8080/users/me"
```

Hooks are resolved during export, so the command is ready to paste. (That also means exporting can hit your auth endpoint — cached per the hook's TTL.)

## Errors

Failures are reported with context, not stack traces:

```
error: invalid line 14 of api.lreq:
  H: X-Loop-Signature "abc..."
  hint: headers use `H: Name = value`

error: request `me` failed:
  field `token` not found in response of hook `login` (available fields: error)
```

The exit code is 1 on any error, so `.lreq` files behave in scripts.

## Roadmap

- `lazyreq import '<curl command>'` — convert a pasted curl (e.g. browser devtools "Copy as cURL") into a request block
- `-v` verbose mode — sent headers, resolved URL, response time
- Assertions (`A: status = 200`) and a test mode for CI
- Environment overlays (`VARS dev` / `VARS prod` + `--env`)

Ideas and PRs welcome.

## Development

```sh
cargo build
cargo run -- example.lreq --list
```

Releases are cut by pushing a `v*` tag — GitHub Actions cross-compiles all targets with GoReleaser + cargo-zigbuild. PRs get a snapshot build as a smoke test.

## License

[MIT](LICENSE)
