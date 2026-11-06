# Connecting an assistant to Candeo

An assistant that speaks the Model Context Protocol — Claude Desktop, Claude
Code, Codex, GitHub Copilot, VS Code, Visual Studio, Cursor — can read Candeo:
the devices and what runs on them, their layouts, the effects library, presets,
automation rules, the names of the signals held, and the installed effects API.
**In this version it changes nothing**: no tool applies, saves or deletes
anything. Why it is shaped this way is [`mcp-server.md`](../design/mcp-server.md);
this is phase 1 of its §9.

## Turning it on

In **Settings › Assistants**:

1. Turn on *Let assistants use Candeo*. It is off until then, and turning it off
   again drops every assistant connected.
2. Choose the assistant, copy the text shown, and paste it where the note says.
   It starts Candeo's own program with `--mcp`, at a path that survives
   updates.
3. Restart the assistant or reload its servers. Settings lists it, by the name
   it gives, once it is connected.

The texts, with `<path>` the program Settings shows:

| Assistant | Where | Text |
|---|---|---|
| Claude Desktop | `claude_desktop_config.json` | `{ "mcpServers": { "candeo": { "command": "<path>", "args": ["--mcp"] } } }` |
| Claude Code | a terminal | `claude mcp add --transport stdio --scope user candeo -- "<path>" --mcp` |
| Codex | the end of `~/.codex/config.toml` | `[mcp_servers.candeo]`, then `command = '<path>'` and `args = ["--mcp"]` on lines of their own |
| GitHub Copilot CLI | `~/.copilot/mcp-config.json` | `{ "mcpServers": { "candeo": { "type": "stdio", "command": "<path>", "args": ["--mcp"], "tools": ["*"] } } }` |
| VS Code, Visual Studio (GitHub Copilot) | VS Code's `mcp.json`; Visual Studio's `%USERPROFILE%\.mcp.json` | `{ "servers": { "candeo": { "type": "stdio", "command": "<path>", "args": ["--mcp"] } } }` |
| Cursor | `~/.cursor/mcp.json` | as Claude Desktop |
| Another | its server settings | the command `<path>`, the argument `--mcp` |

- **Codex**: the Codex CLI, its IDE extension and the ChatGPT desktop app share
  `config.toml`. The table goes at the end, since a TOML table takes every key
  below it up to the next table. The path is a literal string, between single
  quotes, which keeps a Windows path's backslashes; a path holding a single
  quote is a basic string, between double quotes, its backslashes doubled.
  `codex mcp add candeo -- "<path>" --mcp` writes the same table.
- **GitHub Copilot CLI**: the GitHub Copilot app reads the same file, and
  `copilot mcp add candeo -- "<path>" --mcp` writes the same server into it.
  `tools` is required there; `"*"` enables every tool.
- **GitHub Copilot in JetBrains IDEs, Eclipse and Xcode** reads a `servers`
  block too, in the `mcp.json` its chat opens. Its documentation writes a local
  server without `type`: `{ "servers": { "candeo": { "command": "<path>", "args": ["--mcp"] } } }`.
  Not tried.

`<path>` is, by build:

| Build | Program |
|---|---|
| Microsoft Store | `%LOCALAPPDATA%\Microsoft\WindowsApps\candeo.exe`, the package's execution alias |
| MSI, NSIS | the installed `candeo.exe` |
| deb, rpm | the installed `candeo` |
| AppImage | the image's own file |
| macOS | the executable inside the application bundle |

## How it connects

The assistant starts `candeo --mcp`, which starts nothing of Candeo: it connects
to the Candeo already running and copies bytes both ways, until either side
closes.

- **The pipe**: on Windows, a named pipe whose security descriptor grants the
  signed-in user alone, refusing remote clients; on Linux and macOS, a Unix
  socket of mode 0600 in a folder of mode 0700, `$XDG_RUNTIME_DIR/candeo/` or
  `mcp/` in Candeo's data folder. No network port opens, and there is no token
  to copy.
- **Both sides check each other**: Candeo drops a connection of another user,
  and the bridge refuses a pipe another user created.
- **Candeo not running, or MCP off**: the bridge waits 5 seconds, for a Candeo
  still starting at login, then writes *Candeo is not running, or MCP is off in
  its Settings.* to standard error, which assistants show in their server log,
  and exits with code 1. It never starts Candeo.
- **Four assistants at once.** A fifth connects and waits for one of them to
  close.

## The protocol

Candeo implements MCP **2026-07-28**, and answers clients still on an earlier
revision through the `initialize` handshake. The server is named `candeo`. It
offers tools and resources; no prompts yet, and no subscriptions.

**Bounds**: 20 calls a second per connection, beyond which a call is refused
with `mcpTooManyCalls`; messages of 4 MiB at most.

## Tools

Every tool reads, and says so in its annotations (`readOnlyHint`, not
destructive, idempotent, `openWorldHint: false`). Each has an input and an
output schema, and returns `structuredContent` with the same JSON as text.

| Tool | Input | What it gives |
|---|---|---|
| `list_devices` | | every device known, plugged in or not: whether it is controlled, open, split; what runs on it or on each region, with the settings kept for it and the rule interrupting it; whether frames reach it; and whether automations are paused |
| `get_layout` | `device?` | a device's lights — index, matrix row and column, key name, rectangle in key units, shape — its regions and outline; without `device`, the default layout effects are written against |
| `list_effects` | `kind?`: `builtin` or `user` | each effect: id, state, error, description, settings, `apiVersion`, inputs, the lights it suits, what it requires, a sample of its colours |
| `get_effect` | `id` | one effect and its TypeScript source |
| `get_effects_api` | | the installed API's declaration and the guide to writing an effect, as embedded resources, with `apiVersion` and Candeo's version |
| `list_presets` | `effect?` | presets: effect, name, values, bindings |
| `list_automations` | | the rules in priority order, a rule that does not read with its error, the devices each interrupts now, and whether automations are paused |
| `check_automation` | `rule` | whether the rule would be accepted, its next three starts for a cron rule, in local time, and the devices it names that Candeo does not know; nothing is saved |
| `list_signals` | | whether signals are received, and each signal held, by name, with the milliseconds it has left |

A device is written `{ "vid": 5426, "pid": 658 }`, as everywhere in Candeo. An
effect's id is `shipped:<name>` or `user:<name>`.

**Errors.** A refusal, or an argument of the wrong shape, is a tool result with
`isError: true`, whose text is

```json
{ "code": "effectNotFound", "params": { "name": "user:Nothing" }, "message": "No effect is named “user:Nothing”." }
```

`code` is the error's code in Candeo (`errors.<code>` in its catalogs), and
`message` its English text, for the model to correct its call. Only an unknown
tool or a malformed request is a JSON-RPC error.

## Resources

| URI | What it holds | Type |
|---|---|---|
| `candeo://effects-api` | `index.ts` of the installed `@candeo/effects-api`; `apiVersion` and `candeoVersion` in `_meta` | `text/x-typescript` |
| `candeo://effects-api/guide` | how to write an effect for this version | `text/markdown` |
| `candeo://effects-api/example` | the editor's starting point | `text/x-typescript` |
| `candeo://effects/{source}/{name}` | an effect's source; `source` is `shipped` or `user`, the name percent-encoded | `text/x-typescript` |
| `candeo://devices/{vid}-{pid}/layout` | a device's layout, as `get_layout` gives it, VID and PID in four hexadecimal digits: `1532-0292` | `application/json` |
| `candeo://devices/default/layout` | the default layout | `application/json` |
| `candeo://state` | what runs on each device and region, the rules interrupting them, the pause | `application/json` |

Every resource is listed, one per effect and per known device. Each is cached
privately; the API's three for a day, since they change only with Candeo, the
others not at all.

## What it never hands out

- **Signal values.** [`PRIVACY.md`](../../PRIVACY.md) promises they are sent
  nowhere, and an assistant passes what it reads to its model: `list_signals`
  gives names.
- **The error text of an effect that reads key presses or signals.** Such an
  effect writes what it likes into it; MCP says that it failed.
- **The frames of an applied effect**, raw HID access, files outside the effects
  library, `settings.json` as a whole.
- Serial numbers, device instance ids, paths, machine and user names, the copied
  diagnostic.

## The log

Candeo's log names each connection by its number and the assistant by the name
it gives: `assistant connected`, `assistant named itself`, `assistant
disconnected` at `info`, each read at `debug`. Never the pipe's name or path, an
argument, a source or a value; the MCP library itself logs only errors.
