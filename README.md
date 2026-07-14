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
- **Every run is remembered.** An encrypted local history records what was sent and what came back — query it with `--history` instead of re-running requests.
- **Built for AI agents too.** Compact, structured output and persistent memory make it dramatically more token-efficient than curl for LLM workflows ([see below](#lazyreq-for-llm-agents)).
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
lazyreq <file.lreq> --history             # past runs of every request in the file
lazyreq <file.lreq> <request-id> --history  # past runs of one request
lazyreq import '<curl command>'           # convert a curl command to a request block
lazyreq --version                         # print the CLI version
```

```sh
$ lazyreq api.lreq --list
login  POST    $baseURL/login
me     GET     $baseURL/users/me
```

### Importing from curl

Paste a curl command (e.g. browser devtools → "Copy as cURL") and get a request block back:

```sh
$ lazyreq import 'curl "https://api.example.com/v1/users" \
    -H "Authorization: Bearer tok" -d '\''{"name":"yuri"}'\'''
ID: users
POST https://api.example.com/v1/users
H: Authorization = Bearer tok
{"name":"yuri"}
```

Append it to a collection with `lazyreq import '...' >> api.lreq`. Supports `-X`, `-H`, `-d`/`--data*`, `-F` (with `@file` → `file://`), `-u` (→ basic auth header), `-b`, `-A`, `-e` and `--url`; literal `$` in values is escaped automatically.

## Run history

Every executed request — including hook-triggered logins — is recorded into an encrypted history under `~/.lazyreq/history/`, keyed by the file's absolute path. Query it instead of re-running things:

```sh
$ lazyreq api.lreq --history
2026-07-13 21:04:12  login  POST    200     3ms  body:76b  headers:1
2026-07-13 21:04:12  me     GET     200     1ms  body:83b  headers:1

$ lazyreq api.lreq login --history --last 1
2026-07-13 21:04:12  login  POST    200     3ms  body:76b  headers:1
    {token: str(212), user: {email: str(14), id: int}}
```

The single-request view summarizes each response as a **JSON shape** — keys and types instead of values — so you (or an AI agent) can see what an endpoint returns without dumping payloads. Escalate detail only when needed:

```sh
lazyreq api.lreq login --history -v               # resolved URL, request body, full response
lazyreq api.lreq login --history -v --show-headers  # + the actual request headers
lazyreq api.lreq --history --failed               # only non-2xx and transport errors
lazyreq api.lreq --history --status 401           # only a specific status
lazyreq api.lreq --history --success --last 5     # 2xx only, most recent 5
```

Failed sends (DNS, refused connections, timeouts) are recorded too, with the error message in place of a body. The newest 20 runs per request id are kept; `--curl` and `--list` execute nothing and record nothing.

**Encryption.** History and the hook cache are gzipped and encrypted at rest (XChaCha20-Poly1305) with a machine key auto-generated at `~/.lazyreq/key` (mode 0600). Set `LAZYREQ_KEY` to use your own key instead — useful in CI or to share history between machines. Responses often contain tokens and personal data; encrypting them means a synced home directory, a backup, or a stray `cat` can't leak what your APIs returned.

## lazyreq for LLM agents

`.lreq` files were designed to be the API memory an AI coding agent doesn't have. If you let an agent (Claude Code, Cursor, ...) test APIs with raw `curl`, you pay three taxes:

1. **Repetition tax.** Every curl invocation re-states the base URL, headers, auth token and body — hundreds of tokens each time, assembled from scratch. With lazyreq the collection is written once; afterwards a request is `lazyreq api.lreq me` — a handful of tokens, no matter how complex the request.
2. **Auth tax.** With curl, the agent must run the login call, read the token out of the response *into its context window* (where it's now permanently transcribed), and paste it into every following command. lazyreq hooks resolve `$login.token` internally — the token flows from response to header without ever entering the conversation, and the TTL cache means login isn't hammered.
3. **Context-loss tax.** An agent's memory dies with its session (or earlier, when the conversation is compacted). Yesterday's "what did that endpoint return?" is gone, so agents re-run requests — including unsafe POSTs — just to re-learn what they already knew. lazyreq's history survives on disk: a fresh session runs `--history` and gets back status, latency and the response's JSON shape in ~30 tokens, instead of a 3,000-token payload dump or a live re-execution.

The compact-by-default output is deliberate: list views are one line per run, response bodies are summarized as shapes (`{token: str(212), user: {id: int}}`), and full payloads or headers appear only behind explicit flags (`-v`, `--show-headers`). The agent escalates detail only when it needs it.

**Skill.** This repo ships a ready-made skill for Claude Code and compatible agents at [`skills/lazyreq/`](skills/lazyreq/) — it teaches the agent the `.lreq` format, the CLI, and the history-first workflow. Install it by copying (or symlinking) the folder:

```sh
cp -r skills/lazyreq ~/.claude/skills/          # user-wide
# or per project:
cp -r skills/lazyreq your-project/.claude/skills/
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

`$req.<id>` names the request to run; the optional number is a cache TTL in seconds. Cached responses live encrypted in `~/.lazyreq/cache/`, keyed by the file's absolute path + request id, so a login token is fetched once and reused until it expires. Without a TTL the hook runs on every use.

### Requests

| Line | Meaning |
|---|---|
| `ID: name` | starts a request block |
| `DESCRIPTION: text` | optional human-readable description, shown by `--list` and in the editor sidebar |
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

- Assertions (`A: status = 200`) and a test mode for CI
- Environment overlays (`VARS dev` / `VARS prod` + `--env`)

Ideas and PRs welcome.

## Development

```sh
cargo build
cargo test
cargo run -- example.lreq --list
```

Releases are cut by pushing a `v*` tag — GitHub Actions cross-compiles all targets with GoReleaser + cargo-zigbuild. PRs get a snapshot build as a smoke test.

## License

[MIT](LICENSE)
