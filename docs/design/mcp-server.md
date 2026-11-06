# The MCP server: decisions and plan

> **Status: decided. Phase 1 implemented ([#384](https://github.com/o2csi/candeo/issues/384)),
> the others not.** This note answers the
> questions of [`mcp-architecture.md`](mcp-architecture.md) §6 for
> [#359](https://github.com/o2csi/candeo/issues/359): where the server runs, how
> assistants reach it, what the person confirms, and which tools, resources and
> prompts it offers. The transpiler it needs is
> [#381](https://github.com/o2csi/candeo/issues/381), measured in
> [`headless-transpile-poc.md`](headless-transpile-poc.md). Facts about MCP, its
> SDK and its clients were checked on 2026-11-06 against the sources of §12.

**In short.**

- **One server, inside Candeo.** It runs in the process that already owns the
  devices, the engine and `settings.json`, and calls the functions the window's
  commands call. No second process drives the hardware.
- **Reached through `candeo --mcp`.** An assistant starts Candeo's own
  executable with that argument. In that mode it starts nothing: it relays bytes
  between its standard input and output and a local pipe that only the signed-in
  user can open. The assistants checked (Claude Desktop, Claude Code, Codex,
  GitHub Copilot, VS Code, Visual Studio, Cursor) can all start a server this
  way; Claude Desktop has no other way to reach a local one. No TCP port opens,
  and no token is copied anywhere.
- **Off by default**, turned on in *Settings › Assistants*.
- **Candeo enforces confirmation.** Reading, validating, previewing and saving
  something new need none. Changing the lighting, replacing or deleting what the
  person has, and changing rules happen only once the person says yes. They
  answer in the assistant's own dialog (elicitation) when it has one, and in
  Candeo's window otherwise, or always if they prefer. Tool annotations are set,
  and never relied on.
- **21 tools, six kinds of resource, two prompts, in four pull requests**:
  read-only first; then validate, preview and save without hardware, after #381;
  then confirmed changes; then automations.
- **Over the network, fifth**: Streamable HTTP, off by default, TLS on every
  interface but loopback, a token per client, and the same consent, with
  per-token permissions for assistants that cannot ask (§2.7).

## 1. The specification this targets

The current revision is **2026-07-28** ([versioning][s-versioning]); the draft
that follows has no changes yet ([draft changelog][s-draft]). What matters here:

| Topic | What the specification says | Source |
|---|---|---|
| Stateless | No `initialize` handshake and no sessions. Every request carries its protocol version and the client's capabilities in `_meta`. Servers MUST implement `server/discover`. | [changelog][s-changelog] |
| Asking the client | Server-to-client requests are replaced by *multi round-trip requests*. `tools/call`, `prompts/get` and `resources/read` may answer with an `InputRequiredResult`, holding elicitation (or sampling, roots) requests and an opaque `requestState`. The client retries with the answers, under a new request id. | [MRTR][s-mrtr] |
| `requestState` | Attacker-controlled input. Integrity-protected when it matters. A state that must be used at most once MUST be enforced on the server's side. | [MRTR][s-mrtr] |
| Elicitation | Form mode: a flat object of strings, numbers, booleans and enums, answered `accept`, `decline` or `cancel`. Clients MUST show which server asks and offer to decline. Servers MUST NOT send a mode the client did not declare, nor ask for secrets in form mode. | [elicitation][s-elicitation] |
| Tools | `inputSchema`; optional `outputSchema`, then `structuredContent`, also serialised as text. Content is text, image, audio, resource links or embedded resources. Input validation and business errors are results with `isError`, so the model can correct itself; an unknown tool or a malformed request is a JSON-RPC error. Servers MUST validate inputs and rate-limit calls. Annotations are hints, and clients "MUST consider tool annotations to be untrusted". Names: 1 to 128 characters among `A-Z a-z 0-9 _ - .`. | [tools][s-tools] |
| Resources | List, read, URI templates (RFC 6570). Changes reach the client through `subscriptions/listen`, which replaces `resources/subscribe`. List results carry `ttlMs` and `cacheScope`. | [resources][s-resources], [changelog][s-changelog] |
| Prompts | Listed and fetched with arguments; chosen by the user, as slash commands for instance. | [prompts][s-prompts] |
| stdio | The client launches the server as a subprocess. Nothing but MCP messages on stdout; stderr is free for logging. | [stdio][s-stdio] |
| Streamable HTTP | One endpoint taking POST. Servers MUST validate `Origin` and answer 403 to an invalid one. Local servers SHOULD bind to loopback and SHOULD authenticate. | [Streamable HTTP][s-http] |
| Authorization | Optional. Built on OAuth 2.1 for HTTP. Over stdio, a server SHOULD NOT follow it and takes its credentials from the environment. | [authorization][s-auth] |
| Deprecated | Sampling, logging and roots, for twelve months at least. Tasks moved to an extension. | [changelog][s-changelog], [registry][s-deprecated] |

The previous revision, **2025-11-25**, is what many clients still speak: an
`initialize` handshake, and elicitation sent by the server in the middle of a
tool call ([elicitation, 2025-11-25][s-elicitation-old]).

**The SDK.** `rmcp` is the official Rust SDK, in its first tier
([SDKs][s-sdks]). Version 3.9.0, published on 2026-11-05, implements 2026-07-28
and stays compatible with 2025-11-25 ([README][s-rmcp]). It needs Rust 1.88
([crates.io][s-rmcp-crate]), below the 1.97 that #381 sets. Its `server`
feature, on by default, includes `transport-async-rw`: it serves over any byte
stream, a pipe included. `elicitation` and `request-state` are features; the
HTTP server is another, not needed here ([features][s-rmcp-features]). `tokio`,
`tokio-util`, `schemars` 1 and `png` are already in `Cargo.lock`.

**What follows for Candeo.**

- **Confirmation has a place in the protocol.** A writing tool answers its first
  call with an `InputRequiredResult`, and acts only on the retry that carries the
  person's answer. With a 2025-11-25 client, the same form goes out during the
  call. Either way, Candeo acts after the answer.
- **An elicitation answer proves what the client lets through, no more.**
  Claude Code shows a dialog. It also offers an `Elicitation` hook to answer
  "without showing a dialog", and serves elicitation only in interactive
  sessions ([Claude Code][c-claude-code]). Claude Desktop's documentation says
  nothing either way. Hence Candeo's own window, as the fallback and as an
  option (§3.3).
- **Annotations are not consent.** They are set (§3.4) for clients that show
  them, and decide nothing.
- **Sampling and logging are not used.** Candeo calls no model, and it logs to
  its own log.

## 2. Transport and host

### 2.1 What was weighed

Three shapes, against what Candeo has to keep: one process for the hardware and
`settings.json` (`single_instance.rs`), no general network API (#359), off by
default, and the assistants people use.

| | A. Streamable HTTP in Candeo, on loopback, with a token | B. A stdio bridge to the server in Candeo, over a local pipe | C. A stdio server in a process of its own |
|---|---|---|---|
| Who drives the hardware | Candeo | Candeo | a second process: two writers to one keyboard and to `settings.json` (`single_instance.rs`). **Ruled out.** |
| Claude Desktop | **no**: its local servers are commands it starts ([local servers][c-claude-local]). Its custom connectors connect from Anthropic's cloud and need an address reachable from the Internet ([connectors][c-claude-connectors]) | yes | yes |
| Claude Code, VS Code, Cursor | yes, with an `Authorization` header in their configuration | yes | yes |
| Who can reach it | every local process, and every web page: a page can send to any loopback port. The `Origin` check, the `Host` check and the token stand in the way | the signed-in user's processes only: the system checks it when they connect | |
| A secret to copy | a token, in plain text in each client's file (VS Code can prompt for it and store it) | none | |
| What ships | an HTTP stack | a mode of the existing executable, and an execution alias in the MSIX | |
| Candeo not running | connection refused | the bridge says so on stderr, and exits | |

Hypercolor took A: its daemon already serves REST on loopback, and MCP is one
more route on it ([`rgb-controllers.md`](../comparison/rgb-controllers.md)
§6.4). SignalRGB announces A as well, undocumented. Candeo serves no REST, and
#359 asks not to start one for MCP.

### 2.2 Decision: B

**The server runs in Candeo's process and listens on a local pipe. Assistants
start `candeo --mcp`, which relays.**

1. **Every assistant checked** can start a local server as a command, and
   Claude Desktop has no other way. HTTP alone would leave Claude Desktop out.
2. **The system authenticates.** A pipe that only the user's account may open
   lets in the programs that could already write to the effects folder and to
   `settings.json`, the boundary [`effects-library.md`](effects-library.md)
   already draws. A loopback port also lets in every web page. That is why the
   signals API needs a token and refuses `Origin`. It is also why rmcp's HTTP
   server would need configuring: its `Origin` validation is off by default
   ([rmcp][s-rmcp-http]).
3. **Nothing to copy, nothing to renew.** No token sits in an assistant's JSON
   file.
4. **One owner.** The bridge carries bytes. The tools run in Candeo, next to the
   engine, the automations and the single-instance lock.

### 2.3 `candeo --mcp`, the bridge

- **A mode of Candeo's executable, not a second binary.** `main()` reads its
  first argument before `run()`. With `--mcp`, it relays, then exits: Tauri, the
  single-instance plugin, HID and the settings are never touched. A second
  binary would be one more artefact in every bundle format, and it could not
  have a stable path inside an AppImage.
- **What it does.** It connects to the pipe, copies stdin to the pipe and the
  pipe to stdout until either side closes, and exits 0. It parses nothing. The
  protocol, both revisions and the confirmations live in Candeo, so the bridge
  never needs to change with the specification.
- **Candeo not running, or MCP off**: there is no pipe. The bridge retries for
  5 seconds, for a Candeo still starting at login. Then it writes one English
  line to stderr, *Candeo is not running, or MCP is off in its Settings*, and
  exits 1. Assistants show stderr in their server log.
- **It never starts Candeo.** Starting Candeo resumes the applied effects on the
  hardware (`resumeEffects`). An assistant must not cause that.
- **On Windows, the pipe is opened for overlapped I/O**, through tokio's
  named-pipe client. On a handle opened without `FILE_FLAG_OVERLAPPED`, "I/O
  operations are serialized" ([CreateFile][w-createfile]): a thread waiting to
  read would hold up the other thread's writes.
- **No console.** The release build is a Windows GUI program
  (`windows_subsystem = "windows"`). Started by an assistant with redirected
  handles, it reads and writes them all the same, and no console window
  flashes. Tried on 2026-11-06 with the release executable, started as Node
  starts a server, with piped standard streams: it relayed both ways, exited 0
  when its input closed, and with no pipe said so on standard error and exited
  1. No console binary is needed. Not tried through the MSIX alias (§11).

### 2.4 The pipe

- **Windows**: a named pipe. Its name carries the user's SID: named pipes are
  global to the machine, and two people may be signed in. It is created with
  `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, and a DACL
  granting that user alone. The default DACL also grants read access to
  Everyone ([pipe security][w-pipe-security]), hence an explicit one. When the
  first instance cannot be created, something else holds the name: Settings
  says so, and nothing listens. **The bridge checks who serves the pipe**: it
  refuses one whose owner is another account. Anyone may create a named pipe
  of a free name, so while Candeo is not running another person signed in on
  the computer could take this one, read what the assistant sends and answer in
  Candeo's place. One instance always exists while Candeo listens, the next
  created before a client is handed over, so the name is never free between
  two connections.
- **Linux and macOS**: a Unix socket of mode 0600 in a directory only the user
  can open: `$XDG_RUNTIME_DIR/candeo/`, otherwise the application's data
  folder. The peer's uid is checked at each connection, on both sides.
- **Never logged**: the name holds the SID, or a path holding the user name.

### 2.5 Lifecycle

- **Turning MCP on** creates the pipe. **Turning it off** closes it and drops
  every connection, and a confirmation still waiting is refused (§3.2).
  Quitting Candeo does the same: the bridges see the pipe close and exit, and
  the assistant reports the server gone.
- **Several assistants at once**: one pipe instance per connection, four at
  most; a fifth waits for one to close. Each connection is its own rmcp
  service.
- **A connection still counts.** 2026-07-28 has no session, but Candeo knows
  each pipe connection. A pending confirmation belongs to one (§3.2). The log
  names the assistant by the `clientInfo` it sends, which it chooses itself:
  a label, never an authorisation.
- **The development build** shares its identifier with an installed Candeo,
  so only one of them runs (`AGENTS.md`), and both use the same pipe name.
  Either one's bridge reaches whichever runs.

### 2.6 Packaging: a path that survives updates

An assistant's configuration holds a path. It must not change with each update.

| Build | The path given to assistants |
|---|---|
| MSIX (Store) | the execution alias, `%LOCALAPPDATA%\Microsoft\WindowsApps\candeo.exe`. The package's own folder carries its version in its name. `AppxManifest.xml` gains an `AppExecutionAlias` ([alias][w-alias]), which also puts `candeo` on the `PATH`: typed in a terminal, it starts Candeo as the Start menu does. |
| MSI, NSIS | the installed `candeo.exe` |
| deb, rpm | the installed `candeo` |
| AppImage | `$APPIMAGE`, the image's own file rather than where it is mounted |
| macOS | the executable inside the application bundle |

The window resolves the path and shows it (§8). The log does not record it.

### 2.7 Streamable HTTP, for assistants elsewhere (phase 5)

The pipe stays the way in, and nothing above changes. Streamable HTTP is a
second listener of the same server, for an assistant on another computer, or
one that cannot start a command. It is phase 5
([#391](https://github.com/o2csi/candeo/issues/391)), after the
confirmations of phase 3: an assistant reaching Candeo over a network needs the
same consent, and more of it. Decided on 2026-11-06.

#### 2.7.1 Who it is for

| Assistant | Over the network |
|---|---|
| Claude Code, VS Code, Cursor on another computer | yes: each takes a URL and an `Authorization` header ([Claude Code][c-claude-code], [VS Code][c-vscode], [Cursor][c-cursor]) |
| Claude's custom connectors | only at an address reachable from the Internet: they connect from Anthropic's cloud. They take fixed request headers as well as OAuth ([connectors][c-claude-connectors]). Candeo opens nothing to the Internet itself (§2.7.3) |
| An assistant on this computer | the pipe, as before. Loopback HTTP is for one that speaks nothing else |

#### 2.7.2 Listening

- **Off by default**, with a switch of its own beside the pipe's: *Let
  assistants connect over the network*. Turning it off closes the listener and
  every request in flight; a confirmation still waiting is refused.
- **Where**: loopback (`127.0.0.1`, `::1`) always, and the network interfaces
  ticked **by name**, as the signals API does (`signals/interfaces.rs`): an
  interface's address is followed when it changes. Port 7318 by default, the
  one after the signals API's; 1024 or above.
- **One endpoint**, `/mcp`, served by rmcp's `StreamableHttpService` (feature
  `transport-streamable-http-server`). Both revisions, as on the pipe:
  2026-07-28 without sessions; 2025-11-25 through rmcp's legacy sessions
  (`legacy_session_mode`, on by default), each bound to the token that opened
  it.
- **`Host` checked.** rmcp accepts loopback names only, by default
  (`allowed_hosts`). Candeo sets the list to loopback, the ticked interfaces'
  addresses, the names on its certificate and the public name of §2.7.3; any
  other `Host` gets a 403. That is what defeats DNS rebinding, where a page's
  own name is made to resolve to Candeo's address.
- **`Origin` refused.** Servers must validate it and answer 403 to an invalid
  one ([Streamable HTTP][s-http]). rmcp leaves validation off while its list is
  empty; Candeo calls `enforce_origin_validation()` with an empty list, so every
  request carrying an `Origin` gets a 403 ([rmcp][s-rmcp-http]). Assistants are
  not browsers and send none (§11); a page's request always carries one. No
  CORS header is ever sent, and `OPTIONS` is refused.
- **A request passes `Host`, `Origin` and the token (§2.7.4)**, or it reaches
  no tool.

#### 2.7.3 TLS

- **Required on every interface but loopback.** Candeo does not listen on a
  network interface without a certificate: a token sent in clear over a network
  is a token given away. Loopback stays plain HTTP: nothing leaves the computer.
- **A certificate Candeo generates, by default; or yours.**

| | Generated, the default | Yours |
|---|---|---|
| What it is | self-signed, `CA:FALSE`, ECDSA P-256, valid one year. It names `localhost`, the ticked interfaces' addresses, and a host name if one is given | a PEM chain and its key, chosen in Settings: from your own authority, or a public one for a domain you hold |
| What a client needs | to trust it, once: Settings shows its SHA-256 fingerprint and saves the certificate (never the key) as a file to install in the client computer's trust store | what any certificate from that authority needs |
| When an address changes | Settings says the certificate no longer names an address listened on, and offers *New certificate*. Never replaced silently: every client would have to trust it again | Settings says so; renewing it is yours |

- **Generated by default**: a network address has no public name, so no public
  authority will certify it. Without one, Candeo would listen nowhere until the
  person ran an authority.
- **Self-signed rather than a local authority**: a root that may sign makes its
  key able to vouch for any site to every client that trusts it. A certificate
  with `CA:FALSE` vouches for itself alone, whoever reads its key.
- **Yours as well**: whoever already has an authority, or a domain and a public
  certificate, should not have to trust one more.
- **What clients trust.** Claude Code trusts its bundled authorities, the
  system's store and `NODE_EXTRA_CA_CERTS`
  ([network configuration][c-claude-code-network]). VS Code and Cursor were not
  checked (§11). No client checked pins a certificate per server: the
  fingerprint is for the person to compare.
- **The key** is generated on this computer and never leaves it: a file in
  Candeo's configuration folder that the user alone can read (a DACL granting
  the user on Windows, mode 0600 elsewhere). Not `settings.json`, which builds
  share and people copy. A certificate of yours is copied there with its key, so
  that moving the original breaks nothing. Neither the key, nor the
  fingerprint, nor a path reaches the log.
- **Behind a proxy.** Candeo opens nothing to the Internet. Whoever wants an
  assistant to connect from there (Claude's connectors) runs a reverse proxy or
  a tunnel with a public certificate, forwarding to Candeo's loopback listener,
  and gives Settings its public name, which joins the `Host` list. The token is
  still required: a request from loopback proves nothing. `Forwarded` and
  `X-Forwarded-For` are neither trusted nor logged.
- **New dependencies**: a TLS stack (rustls, through tokio-rustls), an HTTP
  server under rmcp's service (hyper), and certificate generation. Their weight
  is measured in the pull request.

#### 2.7.4 Tokens

- **One per client**, created in *Settings › Assistants* with a name the person
  chooses (*Laptop, VS Code*) and its permissions (§2.7.6): 32 random bytes in
  base64url after the prefix `candeo_`, so that a secret scanner recognises one
  pasted where it should not be.
- **Shown once**, with *Copy* and the text to paste (§2.7.8). Afterwards Candeo
  keeps its SHA-256 digest alone: 256 random bits need no slow hash, and the
  digest in `settings.json` gives nothing to whoever reads the file.
- **On every request**, `Authorization: Bearer <token>`, never in the URL: what
  the specification asks of an OAuth access token ([authorization][s-auth]).
  The digest of the token presented is compared with each stored one in
  constant time. A token missing or unknown: 401, with `WWW-Authenticate:
  Bearer` and nothing more.
- **Listed** by name, with its permissions, the day it was created and the day
  it was last used; never its address. 16 at most.
- **Revoked** from the list: its digest is deleted, its requests in flight and
  `subscriptions/listen` streams close, its legacy sessions end, and a
  confirmation it waits for is refused.
- **Not the specification's authorization.** Authorization is optional
  ([authorization][s-auth]). The token is Candeo's own credential, issued and
  checked by Candeo for itself alone, like the signals token. It is what the
  best practices ask of a local server over HTTP: an authorization token, or an
  IPC channel with restricted access ([best practices][s-security]), which is
  the pipe.
- **Failed attempts are limited**: ten a minute from one address, then 429 for
  a minute. Behind a proxy every request comes from loopback, and they share
  the limit. The log counts failures once a minute, with neither the token
  tried nor the address.

#### 2.7.5 OAuth 2.1, when a client requires it

Every assistant checked sends a fixed header: Claude Code, VS Code, Cursor and
Claude's connectors ([Claude Code][c-claude-code], [VS Code][c-vscode],
[Cursor][c-cursor], [connectors][c-claude-connectors]). So tokens come first,
and OAuth is built when an assistant people use offers nothing else; it gets
its issue then. What it takes under 2026-07-28:

| Role | What the specification requires | Source |
|---|---|---|
| Candeo, the resource server | Protected Resource Metadata (RFC 9728), whose `authorization_servers` names at least one; found through `resource_metadata` in the 401's `WWW-Authenticate`, or at the well-known URI. Tokens validated as OAuth 2.1 §5.2 says, and only those issued for Candeo's canonical URI (RFC 8707): no other is accepted or passed on. 401 for an invalid or expired token; 403 with `insufficient_scope` and the scopes the call needs. | [authorization][s-auth], [discovery][s-auth-discovery], [security][s-auth-security] |
| Its authorization server | OAuth 2.1. Metadata through RFC 8414 or OpenID Connect Discovery. Every endpoint over HTTPS. PKCE, with `code_challenge_methods_supported` in the metadata: clients refuse to go on without it. Redirect URIs matched exactly. `iss` in responses (SHOULD, RFC 9207). Short-lived access tokens (SHOULD); refresh tokens rotated for public clients (MUST). | [authorization][s-auth], [security][s-auth-security] |
| Client registration | Client ID Metadata Documents, SHOULD; Dynamic Client Registration, MAY, and deprecated; pre-registration. | [registration][s-auth-registration] |

- **Candeo is its own authorization server.** It has no accounts, and no outside
  server knows who may light this keyboard. Delegating to one the person runs
  would mean validating another issuer's tokens: not planned.
- **The person approves in Candeo's window.** The browser that opens
  `/authorize` may be on another computer, and Candeo cannot tell who sits at
  it. The page shows a short code; Candeo's window shows the client's name, its
  redirect host and the same code, with *Allow* and *Refuse*. Allowing adds an
  entry to the token list, with permissions, revoked the same way.
- **Scopes are the permission classes** of §2.7.6. `scopes_supported` lists
  reading alone; the others come through step-up, when a tool first needs one.
- **Over TLS only, loopback included**: every endpoint of the authorization
  server must be HTTPS ([security][s-auth-security]).
- **Registration.** A Client ID Metadata Document makes Candeo fetch a URL the
  client names: HTTPS only, public addresses only, a size limit, against the
  request forgery the specification warns of ([best practices][s-security]).
  A pre-registered client is entered in Settings: VS Code and Cursor take a
  client id in their configuration ([VS Code][c-vscode], [Cursor][c-cursor]).
  Dynamic Client Registration is not implemented.
- **rmcp brings the transport, not this.** Its `auth` feature is a client's
  ([OAuth support][s-rmcp-oauth]); its example server checks tokens and serves
  both metadata documents in its own code ([example][s-rmcp-auth-example]).

#### 2.7.6 Consent without a screen

On the pipe, the person sits at the computer that runs Candeo. Over a network
they may not: the assistant is elsewhere, and so perhaps are they. A write that
asks (§3.1) is agreed to in one of two ways, and never silently.

1. **The assistant's dialog**, when it declared form elicitation: §3.3 as it
   stands, the description naming the token too (*“Laptop, VS Code” asks: …*).
   The request of §3.2 belongs to the token, since HTTP has no connection: a
   retry presenting another token is `mcpNotConfirmed`. The specification has
   servers bind elicitation to the client and user identity
   ([elicitation][s-elicitation]), and never take a state handle for
   authentication ([best practices][s-security]).
2. **The token's permission**, given in advance, in Candeo's window only.

| Class (§3.1) | Tools | Permissions offered | By default |
|---|---|---|---|
| Read, validate, preview, check | §4.2, §4.3 | always, while the token exists | — |
| Add something new | `save_effect` and `save_preset` under a new name, `rename_preset` | allowed, refused | allowed |
| Change the lighting | `apply_effect`, `stop_effect`, `try_automation` | ask, allowed without asking, refused | ask |
| Replace or delete | `save_effect` and `save_preset` with `replace`, `delete_effect`, `delete_preset`, `delete_automation` | ask, refused | ask |
| Change rules | `save_automation` | ask, refused | ask |

- **Allowed without asking, for the lighting alone.** What it changes shows at
  once and is undone with one gesture. Replacing or deleting loses what the
  person had, and a rule acts later with nobody watching: those ask, or are
  refused. A token for a script that sets the lighting from another computer is
  what this is for.
- **When it should ask and the assistant cannot**, the call is refused at once
  with `mcpCannotAsk`: this assistant cannot ask; allow it on the token, or use
  one that asks. Unless *Confirm in Candeo* is on, it does not wait in Candeo's
  window: nobody may be in front of it, a dialog found an hour later answers
  nothing, and whoever is there may not be whoever asked.
- **Confirm in Candeo** keeps its meaning. On, every write that asks goes to
  Candeo's window, from the pipe and the network alike, and no answer from an
  assistant is taken. *Allowed* and *refused* stand: both were decided in that
  window already.
- **A token sees what it may call.** The tools and resources are the pipe's,
  from the same handler; those a token's permissions refuse are left out of its
  `tools/list`, and refused with `mcpNotPermitted` if called anyway. A change of
  permissions sends `notifications/tools/list_changed` on its listen streams.

#### 2.7.7 Limits and the audit log

- **No tool of its own.** Phase 5 serves what the pipe serves when it ships.
- **Bounds**: 4 MiB per request body, rmcp's `max_request_body_bytes`, its
  default and the pipe's bound; 20 calls a second per token, as per connection
  on the pipe; four requests at once per token, and one listen stream.
  Beyond, 429, or `mcpTooManyCalls` for a tool call.
- **One `info` line per write**, as §3.5 says, and: `http` as the way in; the
  token's name, never the token; where from, `loopback` or `a network
  interface`, never the address, as the signals API logs its listeners
  (`signals/mod.rs`); and how it was agreed to: `elicitation`, `window`,
  `token permission` or `not needed`.
- **Token created, permissions changed, token revoked, certificate generated
  or replaced**: one `info` line each, naming the token. Failed
  authentication: one `warn` line a minute, with a count.

#### 2.7.8 Settings and the window

- **`settings.json`**, in `mcp`: `"http": { "enabled": false, "port": 7318,
  "interfaces": [], "publicName": "", "certificate": "generated", "tokens":
  [{ "name", "digest", "created", "lastUsed", "permissions" }] }`, absent until
  first turned on. The key and the certificates are files beside it (§2.7.3).
- **Settings › Assistants** gains a block, *Over the network*:
  - the switch, the port, and *Listen on*: loopback ticked and disabled, then
    the interfaces by name. When one is ticked, a warning, once: other computers
    on that network can reach Candeo, with a token;
  - the certificate: generated or yours, the names it holds, its fingerprint
    with *Copy*, its expiry; *Save certificate*, *New certificate*, *Use mine*;
  - *Public name*, for a proxy;
  - the tokens, by name, with their permissions, created and last used;
    *New token*; *Revoke*, which asks first.
- **A new token** is shown once, with *Copy* and the text to paste, which also
  says how to trust a generated certificate:

  | Assistant | What to paste |
  |---|---|
  | Claude Code | `claude mcp add --transport http candeo https://<address>:7318/mcp --header "Authorization: Bearer <token>"` |
  | VS Code | in `mcp.json`: `{ "servers": { "candeo": { "type": "http", "url": "https://<address>:7318/mcp", "headers": { "Authorization": "Bearer ${input:candeo-token}" } } }, "inputs": [{ "id": "candeo-token", "type": "promptString", "password": true }] }`. VS Code asks for the token rather than keeping it in the file |
  | Cursor | in `~/.cursor/mcp.json`: `{ "mcpServers": { "candeo": { "url": "https://<address>:7318/mcp", "headers": { "Authorization": "Bearer ${env:CANDEO_TOKEN}" } } } }` |
  | Claude's connectors | through a proxy only: its public URL, and the header under *Request headers* |
  | Another | the URL and the header |

- **For the window**, granted to the main window alone: `get_mcp_http`,
  `set_mcp_http`, `create_mcp_token` (which returns the token, once),
  `set_mcp_token`, `revoke_mcp_token`, `new_mcp_certificate`,
  `use_mcp_certificate`, `save_mcp_certificate`. No MCP tool reaches any of it.
- **Terms**: *token* (FR *jeton*), as the signals API says; *Revoke* (FR
  *Révoquer*).

#### 2.7.9 What it protects against

| Who | What stops them |
|---|---|
| A web page in the person's browser, through DNS rebinding or not | a `Host` not on the list, an `Origin`, no CORS, no token |
| Another computer on a ticked network, without a token | 401; failed attempts limited |
| Someone listening on that network | TLS |
| Someone between the client and Candeo | the client verifying the certificate it was told to trust. A client told to skip verification has no such protection |
| Another account on this computer | the token: loopback, unlike the pipe, is open to every account |
| A leaked token | its permissions; what asks still asks; *Last used* shows it; *Revoke* ends it |

Not protected against: a program running as the person, on either computer,
which can read a token from an assistant's configuration or click in Candeo's
window, the boundary of §3.3; a token allowed to change the lighting doing so;
and whatever a proxy the person opens to the Internet exposes beyond TLS and
the token.

### 2.8 Threads

- rmcp runs on the tokio runtime that Tauri already carries
  (`tauri::async_runtime`).
- **Every call into the domain runs on the main thread**, through
  `run_on_main_thread`, its result sent back. `write_settings` keeps a fixed
  temporary name because synchronous commands run on the main thread, so two of
  them never interleave (`storage.rs`). A tool writing from a tokio thread would
  break that. Reads go the same way: some write back what they learn, and one
  rule is simpler than a list of exceptions.
- **Validation and preview run on a thread of their own**, with #381's stack
  rule and the engine's budgets. Not on the main thread, which has 1 MB of
  stack by default on Windows and runs the window; not on a tokio worker
  either. Oxc needs a large stack, and a long render must not freeze the
  window.

## 3. Consent

### 3.1 What asks

| Action | Asks | Why |
|---|---|---|
| Read anything, validate, preview, check a rule | no | nothing changes, nothing lights |
| Save a new user effect, save a new preset, rename a preset | no | something is added or renamed; nothing is lost, nothing lights |
| Apply an effect, stop one, try a rule | **yes** | the lighting changes (#359) |
| Save over a user effect, replace a preset | **yes** | what the person had is replaced |
| Delete a user effect, a preset or a rule | **yes** | it is gone |
| Create, change, enable or move a rule | **yes** | it changes what the lighting will do later, with nobody watching |

Saving over an effect that is applied changes what its device shows at the
next start: the request says where it is applied.

### 3.2 How Candeo enforces it

For a tool that asks, at every call:

1. **Check** the arguments as the window's command would: the device is known,
   the region valid, the effect `ready`, the name acceptable. A refusal comes
   now, before anyone is asked anything.
2. **Describe** what will happen, in the interface language, from the state at
   this moment: *Apply “Aurora” to DeathStalker V2 Pro? It replaces
   “Breathing”.*
3. **Ask** (§3.3) and **return**, without acting. The request is held in memory:
   a random 128-bit id, the connection, the tool, a digest of the arguments, the
   description, and an expiry two minutes later. One request per connection at a
   time: another writing call meanwhile is refused
   (`mcpConfirmationPending`).
4. **On the answer**, the request is consumed: used once, whatever the answer.
   Accepted, on the same connection, for the same tool and the same arguments,
   before it expires: Candeo acts, through the function the window's gesture
   calls. Otherwise the result is a tool error (`mcpDeclined`,
   `mcpNotConfirmed`) and nothing is done.
5. **Log** the outcome (§3.5).

The arguments are checked again at step 4. If the device was unplugged or the
effect deleted while the person was reading, the action fails as the window's
would.

The `requestState` Candeo hands out is that id and nothing else. Candeo looks it
up rather than trusting what it says, so rmcp's sealed state is not needed, and
consuming it is what makes it used once, as the specification requires of such
a state.

### 3.3 Where the person answers

| | When | How |
|---|---|---|
| **The assistant's dialog** | the assistant declared form elicitation, and *Confirm in Candeo* is off | 2026-07-28: an `InputRequiredResult` with one `elicitation/create` form and the request's id as `requestState`; Candeo acts on the retry. 2025-11-25: the form goes out during the call, and the call waits for it. |
| **Candeo's window** | the assistant declared no elicitation (Claude Code outside an interactive session, an assistant without it), or *Confirm in Candeo* is on | The window comes forward with the request, and for an effect to apply, the simulator previews it. Two buttons: the action's verb, and *Refuse*. The tool call waits, sending progress when the assistant gave a progress token; two minutes without an answer refuses. |

**The form** is the description as its `message` and one required boolean,
`confirm`, titled with the action, with no default. Only `accept` with
`confirm: true` agrees. `decline`, `cancel` and `false` all refuse.

**Confirm in Candeo** is for whoever lets their assistant answer elicitations
on its own, through a hook or an automatic mode. With it on, the only answer
Candeo takes is a click in its own window, which no assistant reaches through
MCP.

Over a network (§2.7.6), a token's permission is a third answer, given in
advance in Candeo's window; and an assistant that cannot ask is refused rather
than left waiting in a window nobody may watch.

What none of this protects against: a program running as the user can write
`settings.json`, or click in the window as a person would. That is the
boundary `effects-library.md` draws for the effects folder, and it stays where
it is.

### 3.4 Annotations

Each tool carries `readOnlyHint`, `destructiveHint` and `idempotentHint` as
they really are (§4), and `openWorldHint: false`: no tool reaches beyond the
computer. An assistant may use them to ask on its own as well. Candeo asks
regardless.

### 3.5 The audit log

One `info` line in Candeo's log for each writing call. It holds the tool; the
target, as VID:PID and region; the ids involved (effect, preset, rule); the
assistant's name as it gave it; how it was confirmed (`elicitation`, `window`,
`not needed`); and the outcome (done, declined, not confirmed, or refused with
its code). In English. No source, no parameter value, no signal, no path.
Reads and previews log at `debug`; connections and disconnections at `info`.
The log already rotates daily and keeps seven files. Over HTTP, the line also
names the token and where the request came from, as a class (§2.7.7).

### 3.6 What MCP never hands out

- **The frames of an applied effect.** They can carry key presses, the sound or
  signal values. Preview frames come from inputs the call gave (§5).
- **The error text of an applied effect that reads keys.** It stays in the
  window ([`key-input.md`](key-input.md) §3); MCP says that the effect failed.
- **Signal values.** [`PRIVACY.md`](../../PRIVACY.md) promises they are "never
  sent anywhere", and an assistant passes what it reads to its model.
  `list_signals` gives names and the time left.
- Serial numbers, device instance ids, paths, machine and user names
  (`AGENTS.md`).
- The copied diagnostic: the person copies it.
- Raw HID access, files outside the user's effects folder, `settings.json` as a
  whole.

## 4. Tools

### 4.1 Conventions

- **Names**: snake_case, a verb first. The assistant adds the server's name.
- **Schemas**: derived from the Rust types with schemars (JSON Schema
  2020-12). Every tool has an `inputSchema` and an `outputSchema`, and returns
  `structuredContent` with the same JSON as text.
- **Errors**: a refusal the window would show, and an argument out of range or
  of the wrong shape, is a tool result with `isError: true` and
  `{ code, params, message }`. `code` is the `Failure` code (`errors.<code>` in
  the catalogs); `message` is its English text, for the model to correct its
  call. Only an unknown tool or a malformed request is a JSON-RPC error, as the
  specification says.
- **Language**: descriptions, results and errors in English. They are read by
  the model, like TypeScript's diagnostics by an effect's author. The
  confirmation's description is in the interface language, through the
  catalogs: the person reads it.
- **Bounds**: a source of 1 MiB at most, as #381 sets; 4 MiB for a message on
  the pipe; 120 frames per preview, one preview at a time per connection; 20
  calls a second per connection, beyond which a call is refused
  (`mcpTooManyCalls`), since the specification asks servers to rate-limit.
  Lists are not paginated: they hold dozens of items.
- **Shared types**:

```ts
DeviceRef = { vid: number, pid: number }

Diagnostic = {
  step: 'size' | 'syntax' | 'unsupported' | 'import' | 'load' | 'render',
  line?: number, column?: number,   // from 1; columns in UTF-16 units, as TypeScript counts
  message: string                   // English
}

ToolError = { code: string, params: Record<string, string>, message: string }
```

### 4.2 Reading: no confirmation

All are read-only and idempotent.

| Tool | Input | Output | The same as |
|---|---|---|---|
| `list_devices` | | per device: `device`, `name`, `present`, `state` (`detected`, `adopted`, `ignored`), `open`, `firmware`, `split`, `regions` (id, name, lights, capabilities); what runs, with its params, bindings and error (withheld for an effect reading keys, §3.6); per region when split; `interruption` (rule, effect, until); `reachingKeyboard`. Then `automationsPaused` | `list_devices`, `engine_status`, `activeEffects` |
| `get_layout` | `device?`; absent, the default layout the preview borrows | `rows`, `cols`, `frameLen`, `keys` (index, row, col, label, rectangle, shape), `regions`, `outline` | `get_layout` |
| `list_effects` | `kind?` (`builtin`, `user`) | per effect: `id`, `name`, `kind`, `state`, `error`, `description`, `params` (as `ParamSpec`), `apiVersion`, `inputs`, `lights`, `swatch` | `list_effects` |
| `get_effect` | `id` | the entry and its `source` | `read_effect_source` |
| `get_effects_api` | | the API's declaration and the guide (§6) as embedded resources, with `apiVersion` and Candeo's version | for assistants whose model cannot read resources itself |
| `list_presets` | `effect?` | presets: effect, name, values, bindings | `presets` in `settings.json` |
| `list_automations` | | `paused`; the rules in priority order, in the shape of `Rule`, a rule that does not read given with its error; which apply now | `rules`, `engine_status` |
| `check_automation` | `rule` | `ok`, `error`; for a cron rule, its next three starts in local time; the devices it names that Candeo does not know | the check of `set_rules`, without saving |
| `list_signals` | | whether reception is on; each signal held, by name, with the time left (no value, §3.6) | `list_signals` |

### 4.3 Validating and previewing: no confirmation, no hardware

| Tool | Input | Output |
|---|---|---|
| `validate_effect` | `source`, `device?` | `ok`, `diagnostics`, and when the module loads, its manifest: `description`, `params`, `apiVersion`, `inputs`, `lights`. Runs #381's contract: size, syntax and early errors, refused constructs, imports, load, a few frames. Writes nothing, caches nothing. |
| `preview_effect` | `source`, or `effect` (an id); `device?`, `region?`, `params?`, `bindings?`; `times?`, in ms, 120 at most, by default 0, 250, 500, 1000 and 2000; `inputs?`, simulated: presses, clock, signals, sound, system; `image?`, `sheet` (the default) or `none` | `frames`: for each time, one `#rrggbb` per light, in the layout's order. `summary`: a swatch, the share of lights lit, the share changing between frames. `diagnostics`, runtime errors located in the source through the source map. And a PNG contact sheet (§5). |

### 4.4 Writing

| Tool | Input | Asks | Output | The same as |
|---|---|---|---|---|
| `save_effect` | `name`, `source`, `replace?` | only with `replace`, over an existing user effect | `id`, `hash`, `state`, `diagnostics` | `save_effect_source`, then the compilation of #381 |
| `delete_effect` | `id`, a user effect | yes: says where it is applied and how many presets go with it | | `delete_effect` |
| `apply_effect` | `device`, `region?`, `effect`, `params?`, `bindings?`, `preset?` | yes: says what it replaces, and which rule's interruption it ends | what runs now, `reachingKeyboard` | the window's *Apply*: `start_effect`, then remembered |
| `stop_effect` | `device`, `region?` | yes | what runs now | `stop_effect` |
| `save_preset` | `effect`, `name`, `values`, `bindings?`, `replace?` | only with `replace` | the effect's presets | `save_preset`, `update_preset` |
| `rename_preset` | `effect`, `from`, `to` | no | the effect's presets | `rename_preset` |
| `delete_preset` | `effect`, `name` | yes | the effect's presets | `delete_preset` |
| `save_automation` | `rule`, replacing the rule of the same `id` if there is one; `position?` | yes: says when it applies next | the rules | `set_rules`, the list changed |
| `delete_automation` | `id` | yes | the rules | `set_rules` |
| `try_automation` | `id` | yes | what runs now | `try_rule` |

- **`save_effect` refuses a source that does not load** (`effectInvalid`, with
  its diagnostics): an effect added through MCP runs. As in the window, a user
  effect may share a shipped effect's name, and a shipped effect is never saved
  over.
- **`apply_effect`** is the window's gesture, so it ends a rule's interruption
  on that device, as any gesture does
  ([`commands.md`](../api/commands.md#automations)), and the request says so.
  `preset` applies the effect with that preset's values ([`presets.md`](presets.md)).
- **The window follows.** Today it re-reads after its own gestures only. Each
  write emits `candeo://etat-change` for what runs, and a new
  `candeo://library-changed` naming what changed: `effects`, `presets` or
  `rules`.

### 4.5 Not offered, and why

| | Why not |
|---|---|
| Sending a signal | A signal's sender states a fact about the world ([`inputs-and-automations.md`](inputs-and-automations.md) §2.3). An assistant tests a rule with `try_automation`; scripts keep the signals API. |
| Game state | Games arrive as signals, and `list_signals` names them when games are on. |
| The frames of an applied effect; raw HID | §3.6. |
| Firmware effects (`set_effect`), brightness | They stay in the window for now. Each would be one more confirmed write, if asked for. |
| Settings, the diagnostic, the log | Not lighting. The diagnostic is the person's to send. |
| Pausing automations | One switch in the tray and in the window; nothing an assistant needs to compose lighting. |
| Layered composition | #360's layers do not exist yet. When they do, a tool takes the same data as the window. |
| A type check | Optional per [`headless-transpile-poc.md`](headless-transpile-poc.md) §5: 0.4 to 0.7 s and up to a few hundred MB. Later, as `check_effect_types`, if assistants need it. |

## 5. Preview

- **Not the window's preview loop.** There is one (`start_preview`), and the
  window draws it: an assistant previewing would replace what the person is
  looking at. `preview_effect` renders apart. It loads the module in a new
  QuickJS context, renders the times asked for one after the other, and drops
  the context. That is the path the swatch already takes (`prepare_bounded`,
  then `render_once`, in `runtime/swatch.rs`), under the same budgets.
- **Simulated inputs only.** The real keyboard, sound, signals and computer are
  never read, and no key capture starts ([`key-input.md`](key-input.md) §3):
  what comes back depends on the call alone. A press is a light's index and a
  time.
- **The image is drawn from the layout's own geometry.** Rectangles, shapes
  (rectangle, disc, arch) and the outline are in the device's definition, in
  `candeo-device`. The simulator draws them, and the engine hands the same
  rectangles to effects ([`studio.md`](studio.md) §4). A Rust function draws
  the same data into pixels. So the geometry is not copied, only the few lines
  that fill a rectangle, a disc and an arch: this answers that question of
  `mcp-architecture.md` §6. No key labels, no maker's artwork. The contact sheet
  sets the frames side by side, each captioned with its time, at most 1 200
  pixels wide. It is a PNG, through the `png` crate already in the tree.
- **Bounded**: 120 times and 60 s of effect time at most; the engine's load,
  frame and memory budgets; one preview at a time per connection; stopped when
  the assistant cancels the call.

## 6. Resources

| URI | What it holds | Type | Listed |
|---|---|---|---|
| `candeo://effects-api` | `index.ts` of the installed `@candeo/effects-api`: every type and helper, with its documentation. `apiVersion` and Candeo's version in `_meta`. | `text/x-typescript` | yes |
| `candeo://effects-api/guide` | How to write an effect for this version: the module's shape, its one import, inputs, lights, limits, what validation checks, and the workflow (validate, preview, save, then ask before applying). Written once in the repository; a test validates its examples. | `text/markdown` | yes |
| `candeo://effects-api/example` | `example.ts`, the editor's starting point | `text/x-typescript` | yes |
| `candeo://effects/{source}/{name}` | an effect's source; `source` is `shipped` or `user`, as in the effect's id. The 26 shipped effects are the examples. Names are percent-encoded. | `text/x-typescript` | each effect |
| `candeo://devices/{vid}-{pid}/layout` | a device's layout, as `get_layout` gives it; VID and PID in four hexadecimal digits each | `application/json` | each known device, and `candeo://devices/default/layout` |
| `candeo://state` | what runs on each device and region, interruptions, the pause | `application/json` | yes |

- **One function each.** A resource and the tool that gives the same thing
  return what one function builds. Resources serve assistants that attach
  context; tools serve models that fetch it themselves. The guide and the
  declaration are not a copy of the API: they are the installed files.
- **Versions.** The API's resources carry `apiVersion`, 1 today, and Candeo's
  version. An assistant adapts to the version installed, never to the
  repository's.
- **Caching.** `cacheScope: "private"` everywhere. `ttlMs` is 0 for the state,
  and longer for the API, which changes only with Candeo.
- **Changes**, through `subscriptions/listen`: `resourcesListChanged` when the
  library or the devices change, and `resourceSubscriptions` on
  `candeo://state` (phase 4).

## 7. Prompts

| Prompt | Arguments | What it sets up |
|---|---|---|
| `create_effect` | `description`; `device?`, written as the log writes a device, `0xVVVV:0xPPPP` | The guide, the API and the device's layout, embedded. The loop: write, validate, preview, look at the sheet, correct, save under a new name, then offer to apply, which asks the person. |
| `create_automation` | `description` | The rules' shape, the current rules and the devices. Check the rule, then save it, which asks. |

In English: the model reads them. A third one, if asked for, would adapt an
effect to another device.

## 8. Settings and the window

- **`settings.json`**: `"mcp": { "enabled": false, "confirmInCandeo": false }`.
  A build released before #373 drops it when it writes the file (`AGENTS.md`):
  MCP is then off, the safe side.
- **Settings › Assistants**, after *Signals*:
  - **A switch**, *Let assistants use Candeo*, off by default.
  - **The state**: off; on, with no assistant connected; on, with the assistants
    connected, by the names they give. A pipe that could not be created says
    so.
  - **Confirm in Candeo**, a switch, off by default (§3.3).
  - **Connect an assistant**: a choice of assistant, and the text to paste, with
    *Copy*, holding the path of §2.6:

    | Assistant | What to paste |
    |---|---|
    | Claude Desktop | in `claude_desktop_config.json`: `{ "mcpServers": { "candeo": { "command": "<path>", "args": ["--mcp"] } } }` |
    | Claude Code | `claude mcp add --scope user candeo -- "<path>" --mcp` |
    | Codex | at the end of `~/.codex/config.toml`: `[mcp_servers.candeo]`, `command = '<path>'`, `args = ["--mcp"]` |
    | GitHub Copilot CLI | in `~/.copilot/mcp-config.json`: `{ "mcpServers": { "candeo": { "type": "stdio", "command": "<path>", "args": ["--mcp"], "tools": ["*"] } } }` |
    | VS Code, Visual Studio (GitHub Copilot) | in `mcp.json`, or Visual Studio's `%USERPROFILE%\.mcp.json`: `{ "servers": { "candeo": { "type": "stdio", "command": "<path>", "args": ["--mcp"] } } }` |
    | Cursor | in `~/.cursor/mcp.json`: as for Claude Desktop |
    | Another | the command and its argument |

    Each format comes from the assistant's documentation (§12), and is checked
    again when implementing. One entry per format, not per product: the
    entry names every program that reads the same text. Codex and GitHub
    Copilot CLI get the lines of their file rather than their `mcp add`
    command, because their desktop apps and editor extensions read that file
    and may be installed without the command.
- **Turning it off is the revocation.** The pipe closes, connections drop, a
  request waiting is refused. An assistant's configuration may stay; it then
  gets the bridge's message.
- **No pairing step, no token**, on the pipe. The switch is the person's
  consent to let assistants connect; each change is consented to on its own.
  Over the network, tokens (§2.7.4).
- **The request in the window**: a dialog over the current screen, naming the
  assistant and saying what will happen, with the action's verb and *Refuse*.
  For an effect to apply, the simulator previews it.
- **Terms**, settled here: *assistant* (FR *assistant*) for an MCP client, and
  *Refuse* (FR *Refuser*) for turning a request down. New catalog keys go under
  `settings.assistants` and `assistants`.
- **For the window**: `get_mcp()`, `set_mcp(enabled, confirmInCandeo)`,
  `answer_mcp_request(id, accepted)`, and the events `candeo://mcp-request` and
  `candeo://library-changed`. Only the main window may call
  `answer_mcp_request`: the capability grants its commands to that window
  alone.
- **[`PRIVACY.md`](../../PRIVACY.md)** gains *Assistants, if you turn them on*:
  what an assistant can read and change, that what it reads goes to the
  assistant and to whatever service it uses, which Candeo does not choose, and
  what is never handed out (§3.6).

## 9. Phases

Five pull requests, in this order, each usable on its own. Each one documents
its part in `docs/api/mcp.md`, a new reference next to
[`signals.md`](../api/signals.md), and adds its *What's new* line in English and
French. Each is tried with a real assistant in the running app before review
(`AGENTS.md`).

| Phase | Issue | What | Depends on |
|---|---|---|---|
| 1. The server, read-only | [#384](https://github.com/o2csi/candeo/issues/384) | The server in Candeo, the pipe, `candeo --mcp`, the MSIX alias; *Settings › Assistants*; the reading tools (§4.2) and the resources (§6) without subscriptions; the connections logged; `PRIVACY.md`, the README, the Store listings and the site. `rust-version` raised to 1.88 if #381 has not raised it yet. | — |
| 2. Validate, preview, save | [#385](https://github.com/o2csi/candeo/issues/385) | `validate_effect`, `preview_effect` and its contact sheet, `save_effect` and `save_preset` for new names (`replace` refused until phase 3), `rename_preset`, the `create_effect` prompt, the guide's test, `candeo://library-changed`. An end-to-end test through rmcp's client and no device: a broken effect, its diagnostic, the fix, a preview, a save. It meets #365's acceptance. | #384, #381 |
| 3. Confirmed changes | [#386](https://github.com/o2csi/candeo/issues/386) | Consent (§3): requests, elicitation in both revisions, the window's dialog, *Confirm in Candeo*, the audit log. `apply_effect`, `stop_effect`, `save_effect` with `replace`, `delete_effect`, `save_preset` with `replace`, `delete_preset`. | #385 |
| 4. Automations | [#387](https://github.com/o2csi/candeo/issues/387) | `save_automation`, `delete_automation`, `try_automation`, the `create_automation` prompt, subscriptions to `candeo://state` and to the lists. | #386 |
| 5. Over the network | [#391](https://github.com/o2csi/candeo/issues/391) | Streamable HTTP (§2.7): the listener, off by default, on loopback and the interfaces ticked; `Host` and `Origin` checked; TLS off loopback, generated or yours; tokens per client, created, listed and revoked in *Settings › Assistants*; per-token permissions and `mcpCannotAsk`; the audit line per write. The tools and resources the pipe has by then. OAuth 2.1 (§2.7.5) later, in its own issue, once an assistant needs it. | #386 |

Phases 1 and 2 need no confirmation: they change nothing that lights, and
overwrite nothing. Phase 5 can follow phase 3 or 4: it adds no tool.

## 10. Not chosen

- **HTTP first** (§2.1).
- **A second binary** for the bridge (§2.3).
- **The bridge starting Candeo** (§2.3).
- **A token, or a pairing code, on the pipe.** HTTP needs them (§2.7.4). On a
  pipe, the system already says who connects: a token in an assistant's JSON
  file would add a secret to leak, and nothing to that check.
- **Confirmation left to the assistant**, as Hypercolor does. Annotations are
  hints, and #359 asks for an affirmative action Candeo can enforce.
- **A grant for a while** (*allow for ten minutes*). Fewer questions, but a
  stretch of time during which nothing is asked. To revisit if confirmations
  prove tiring; the authoring loop, through preview, needs none. Over the
  network, the one grant is a token's, for the lighting alone, set in Candeo's
  window (§2.7.6).
- **Plain HTTP on a network interface**, **a local certificate authority**, and
  **Candeo obtaining a public certificate itself** (ACME), which needs a domain
  and a challenge a home network cannot answer (§2.7.3).
- **Trusting `X-Forwarded-For`** behind a proxy (§2.7.3).
- **A remote request waiting in Candeo's window** when the assistant cannot ask,
  unless *Confirm in Candeo* is on (§2.7.6).
- **Replacing, deleting or changing rules without asking**, even on a token
  (§2.7.6).
- **Dynamic Client Registration**, deprecated by the specification, and **an
  outside authorization server** (§2.7.5).
- **Sampling**, to have Candeo write effects itself: deprecated, and #359
  rules out a model Candeo would call on its own.
- **Tasks**: a preview is bounded to seconds.

## 11. Not verified

- **A Windows GUI executable as a stdio server**: works when started with
  piped standard streams, as Node starts one (§2.3). Not tried through the
  MSIX execution alias, nor with each assistant. If an assistant fails to read
  it, the bridge becomes a small console binary installed next to Candeo.
- **Elicitation in Claude Desktop, VS Code and Cursor.** The documentation of
  VS Code and Cursor lists it; Claude Desktop's says nothing. None was tried,
  nor which revision each assistant speaks.
- **How each assistant draws a form of one boolean**, and whether it shows the
  message in full: phase 3.
- **Whether assistants keep a tool call open for two minutes** while the window
  waits. Claude Code's default tool timeout is about 28 hours; the others were
  not checked.
- **rmcp 3.9.0 against these needs**: serving over a pipe, both revisions on
  that one transport, an `InputRequiredResult` from a tool, elicitation during a
  call for 2025-11-25 clients. Read in its documentation, not built.
- **The named pipe's DACL and name inside the MSIX**, and the Unix socket on
  macOS.
- **Whether a model makes good use of the contact sheet**: measured in phase 2.
- **That the contact sheet and the simulator look alike.** Both read one
  geometry; no test compares the two drawings.
- **Over the network** (phase 5), none of it tried:
  - that Claude Code, VS Code and Cursor send no `Origin` over HTTP. One that
    did would be refused, and its `Origin` would have to be allowed by name;
  - how VS Code and Cursor trust a certificate the system does not, and whether
    Claude Code's system store and `NODE_EXTRA_CA_CERTS`, documented for its
    network configuration, apply to its MCP connections too;
  - that every client accepts the generated certificate once trusted: ECDSA
    P-256, `CA:FALSE`, one year;
  - rmcp's legacy sessions bound to a token: its session store knows no token,
    so Candeo's layer would keep the pairing;
  - the weight of rustls, hyper and certificate generation in the binary;
  - whether any assistant people use needs OAuth.

## 12. Sources

Checked on 2026-11-06.

- **MCP specification**, revision 2026-07-28 unless said otherwise:
  [versioning][s-versioning], [changelog][s-changelog],
  [draft changelog][s-draft], [deprecated features][s-deprecated],
  [multi round-trip requests][s-mrtr], [elicitation][s-elicitation],
  [elicitation in 2025-11-25][s-elicitation-old], [tools][s-tools],
  [resources][s-resources], [prompts][s-prompts], [stdio][s-stdio],
  [Streamable HTTP][s-http], [authorization][s-auth],
  [authorization server discovery][s-auth-discovery],
  [client registration][s-auth-registration],
  [authorization security][s-auth-security],
  [security best practices][s-security], [SDKs][s-sdks].
- **rmcp**: [repository and README][s-rmcp], [crates.io][s-rmcp-crate] (3.9.0,
  2026-11-05, Rust 1.88), [features][s-rmcp-features],
  [`StreamableHttpServerConfig`][s-rmcp-http] (its source in 3.9.0 for the
  defaults: `allowed_hosts` loopback, `allowed_origins` empty and validation
  off, `max_request_body_bytes` 4 MiB, `legacy_session_mode` on),
  [OAuth support][s-rmcp-oauth], [example server with OAuth][s-rmcp-auth-example].
- **Assistants**: [connecting local servers to Claude Desktop][c-claude-local],
  [Claude's custom connectors][c-claude-connectors],
  [Claude Code and MCP][c-claude-code],
  [Claude Code's network configuration][c-claude-code-network],
  [VS Code's `mcp.json`][c-vscode], [VS Code's MCP support][c-vscode-api],
  [Cursor and MCP][c-cursor], [Codex and MCP][c-codex],
  [GitHub Copilot CLI's MCP servers][c-copilot-cli] and
  [its configuration fields][c-copilot-cli-reference],
  [Copilot Chat's MCP servers in each editor][c-copilot-ide],
  [Visual Studio's MCP servers][c-visual-studio].
- **Windows**: [`CreateFileW`][w-createfile],
  [named pipe security][w-pipe-security],
  [`uap5:AppExecutionAlias`][w-alias].

[s-versioning]: https://modelcontextprotocol.io/specification/versioning
[s-draft]: https://modelcontextprotocol.io/specification/draft/changelog
[s-changelog]: https://modelcontextprotocol.io/specification/2026-07-28/changelog
[s-deprecated]: https://modelcontextprotocol.io/specification/2026-07-28/deprecated
[s-mrtr]: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr
[s-elicitation]: https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation
[s-elicitation-old]: https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation
[s-tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools
[s-resources]: https://modelcontextprotocol.io/specification/2026-07-28/server/resources
[s-prompts]: https://modelcontextprotocol.io/specification/2026-07-28/server/prompts
[s-stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
[s-http]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
[s-auth]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
[s-auth-discovery]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery
[s-auth-registration]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
[s-auth-security]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations
[s-security]: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices
[s-sdks]: https://modelcontextprotocol.io/docs/sdk
[s-rmcp]: https://github.com/modelcontextprotocol/rust-sdk
[s-rmcp-crate]: https://crates.io/crates/rmcp
[s-rmcp-features]: https://docs.rs/crate/rmcp/latest/features
[s-rmcp-http]: https://docs.rs/rmcp/latest/rmcp/transport/streamable_http_server/tower/struct.StreamableHttpServerConfig.html
[s-rmcp-oauth]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md
[s-rmcp-auth-example]: https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/complex_auth_streamhttp.rs
[c-claude-local]: https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-local-servers
[c-claude-connectors]: https://support.claude.com/en/articles/11175166
[c-claude-code]: https://code.claude.com/docs/en/mcp
[c-claude-code-network]: https://code.claude.com/docs/en/network-config
[c-vscode]: https://code.visualstudio.com/docs/agents/reference/mcp-configuration
[c-vscode-api]: https://code.visualstudio.com/api/extension-guides/ai/mcp
[c-cursor]: https://cursor.com/docs/context/mcp
[c-codex]: https://learn.chatgpt.com/docs/extend/mcp?surface=cli
[c-copilot-cli]: https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers
[c-copilot-cli-reference]: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference#mcp-server-configuration
[c-copilot-ide]: https://docs.github.com/en/copilot/how-tos/copilot-in-your-ide/customize-copilot/extend-copilot-with-tools-and-context/extend-copilot-chat-with-mcp
[c-visual-studio]: https://learn.microsoft.com/en-us/visualstudio/ide/mcp-servers
[w-createfile]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
[w-pipe-security]: https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
[w-alias]: https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap5-appexecutionalias
