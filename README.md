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

## Interactive UI

Run `lazyreq` with no arguments to open a lazygit-style terminal UI. It scans the current directory for `.lreq` files (`--path <dir>` scans anywhere, e.g. `lazyreq --path ~/projects`):

```
┌[1] files ────────────┐┌[2] requests — api.lreq ─────────────────────────┐
│ api.lreq             ││┌login───────────────┐┌me──────────────────┐     │
│ ~/projects/lazyreq   │││POST $baseURL/login ││GET $baseURL/users/me│    │
│                      │││200 · 3ms · 21:04   ││not run yet         │     │
│ shop.lreq            ││└────────────────────┘└────────────────────┘     │
│ ~/projects/shop      │└─────────────────────────────────────────────────┘
│┌shortcuts───────────┐│┌[3] history — login ─────────────────────────────┐
││ 1/2/3  jump panel  │││ ⠼ running…   login                              │
││ ⏎  open/run/view   │││ 0853d0e6 07-14 21:04 login POST 200 3ms {token:.│
││ r  retry run       │││ 4645a373 07-14 21:02 login POST 401 2ms {error:.│
│└────────────────────┘│└─────────────────────────────────────────────────┘
└──────────────────────┘ ⏎ run · j/k move · tab panel · ? keys · q quit
```

- **[1] files** — every `.lreq` found (with its directory); parse errors are marked and shown.
- **[2] requests** — cards with method, URL and the last run's status/latency from history.
- **[3] history** — recorded runs (all files while browsing; the selected request's runs once one is focused). Runs appear here live with a spinner while executing. `⏎` opens the full detail (request/response), `r` retries a run — exact recorded body, fresh auth.
- Navigation is lazygit-flavored: `1/2/3` jump between panels, `tab` cycles, `hjkl`/arrows move, `?` shows all keybindings. View/run only for now — editing comes later.

### Themes

The UI ships five built-in themes — `default` (the lazyreq logo palette), `dracula`, `solarized-dark`, `solarized-light` and `atom` — with background and foreground forced, so it looks the same on any terminal scheme. Press `t` to cycle themes; the choice persists.

Themes live in `~/.lazyreq/themes.json` (created on first run with every built-in, so the format is discoverable). Edit a theme, add your own, or set `current`:

```json
{
  "current": "default",
  "themes": {
    "mine": {
      "background": "#160f09",
      "base": "#e6d7bf",
      "primary": "#bb671f",
      "secondary": "#9a5619",
      "border": "#613614",
      "running": "#8b7b2e",
      "muted": "#8a7963",
      "selection_text": "#1d140b"
    }
  }
}
```

Missing fields fall back to the `default` palette; user themes join the `t` cycle.

## CLI

```sh
lazyreq                                   # interactive UI (scans the current directory)
lazyreq --path <dir>                      # interactive UI over another directory tree
lazyreq <file.lreq> <request-id>          # run a request
lazyreq <file.lreq> <request-id> --curl   # print it as a curl command instead
lazyreq <file.lreq> --list                # list every request in the file
lazyreq <file.lreq> --history             # past runs of every request in the file
lazyreq <file.lreq> <request-id> --history  # past runs of one request
lazyreq <file.lreq> --retry <run-id>      # replay a recorded run (exact body, fresh auth)
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
0853d0e6  2026-07-13 21:04:12  login  POST    200     3ms  body:76b  headers:1
4645a373  2026-07-13 21:04:12  me     GET     200     1ms  body:83b  headers:1

$ lazyreq api.lreq login --history --last 1
0853d0e6  2026-07-13 21:04:12  login  POST    200     3ms  body:76b  headers:1
    {token: str(212), user: {email: str(14), id: int}}
```

The first column is the **run id** — a unique id per execution, distinct from the request's `ID:` name.

The single-request view summarizes each response as a **JSON shape** — keys and types instead of values — so you (or an AI agent) can see what an endpoint returns without dumping payloads. Escalate detail only when needed:

```sh
lazyreq api.lreq login --history -v               # resolved URL, request body, full response
lazyreq api.lreq login --history -v --show-headers  # + the actual request headers
lazyreq api.lreq --history --failed               # only non-2xx and transport errors
lazyreq api.lreq --history --status 401           # only a specific status
lazyreq api.lreq --history --success --last 5     # 2xx only, most recent 5
lazyreq api.lreq --history --req 0853d0e6 -v      # one specific run, by run id
```

### Retrying a run

```sh
lazyreq api.lreq --retry 0853d0e6
```

`--retry` replays a recorded run: the **recorded URL and body are sent verbatim** — including generated `$uuid()` / `$fuzz_*()` values, which a normal re-run would regenerate — while **headers are re-resolved** from the current request definition, so auth hooks produce fresh tokens and `$hmac($body, ...)` signatures are recomputed over the replayed body. That makes it ideal for reproducing a failing request exactly. The retry is recorded as a new run with its own run id.

Failed sends (DNS, refused connections, timeouts) are recorded too, with the error message in place of a body. The newest 20 runs per request id are kept; `--curl` and `--list` execute nothing and record nothing.

**Encryption.** History and the hook cache are gzipped and encrypted at rest (XChaCha20-Poly1305) with a machine key auto-generated at `~/.lazyreq/key` (mode 0600). Set `LAZYREQ_KEY` to use your own key instead — useful in CI or to share history between machines. Responses often contain tokens and personal data; encrypting them means a synced home directory, a backup, or a stray `cat` can't leak what your APIs returned.

## lazyreq for LLM agents

`.lreq` files were designed to be the API memory an AI coding agent doesn't have. If you let an agent (Claude Code, Cursor, ...) test APIs with raw `curl`, you pay three taxes:

1. **Repetition tax.** Every curl invocation re-states the base URL, headers, auth token and body — hundreds of tokens each time, assembled from scratch. With lazyreq the collection is written once; afterwards a request is `lazyreq api.lreq me` — a handful of tokens, no matter how complex the request.
2. **Auth tax.** With curl, the agent must run the login call, read the token out of the response *into its context window* (where it's now permanently transcribed), and paste it into every following command. lazyreq hooks resolve `$login.token` internally — the token flows from response to header without ever entering the conversation, and the TTL cache means login isn't hammered.
3. **Context-loss tax.** An agent's memory dies with its session (or earlier, when the conversation is compacted). Yesterday's "what did that endpoint return?" is gone, so agents re-run requests — including unsafe POSTs — just to re-learn what they already knew. lazyreq's history survives on disk: a fresh session runs `--history` and gets back status, latency and the response's JSON shape in ~30 tokens, instead of a 3,000-token payload dump or a live re-execution.

The compact-by-default output is deliberate: list views are one line per run, response bodies are summarized as shapes (`{token: str(212), user: {id: int}}`), and full payloads or headers appear only behind explicit flags (`-v`, `--show-headers`). The agent escalates detail only when it needs it.

### Benchmarks

Measured with [`bench/bench.py`](bench/bench.py): real `curl` and `lazyreq` subprocesses drive a live local API through agent workflows, and everything the agent must **write** (commands) and **read** (output) is counted with tiktoken. Reproduce with `python3 bench/bench.py` (needs `lazyreq` on `PATH`; `pip install tiktoken` for exact counts).

| workflow | curl | lazyreq | savings |
|---|--:|--:|--:|
| author the `.lreq` collection (one-time) | 0 | 368 | — |
| cold start: login + authenticated GET | 563 | 198 | **65%** |
| repeat the same GET ×5 | 1,605 | 990 | **38%** |
| new session: recall what an endpoint returns | 1,669 | 157 | **91%** |
| debug: inspect a failing POST | 658 | 142 | **78%** |
| reproduce a failed run exactly (fuzzed payload) | 1,007 | 114 | **89%** |
| send an HMAC-signed webhook | 206 | 42 | **80%** |
| recover from an expired auth token | 757 | 198 | **74%** |
| **total** | **6,465** | **2,209** | **66%** |

Weighted by price (generated tokens cost ~5× ingested ones), the overall saving is **76%** — lazyreq's advantage concentrates in the expensive direction: a full authenticated flow is 7 generated tokens instead of 225.

Recall cost is where the design shows most. Asking "what does this endpoint return?" in a fresh session costs curl a re-auth plus the full payload; lazyreq answers from history with a constant-size shape summary:

| response size | curl | lazyreq | savings |
|---|--:|--:|--:|
| /orders, 5 items (1.7 KB) | 1,060 | 159 | 85% |
| /orders, 50 items (16.5 KB) | 6,565 | 159 | 98% |
| /orders, 200 items (66.5 KB) | 25,015 | **159** | **99%** |

curl's recall grows linearly with the payload; lazyreq's stays flat at 159 tokens. At 200 items, a single curl recall costs more than the entire eight-workflow lazyreq suite — and the one-time authoring cost (368 tokens) is repaid about twice over by the first workflow.

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
  H: X-Signature "abc..."
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
