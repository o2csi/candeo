# Candeo — effect architecture and an agent interface (proposal)

> **Status: proposal, not implemented.** This note tells what the repository does
> today apart from the design considered. It complements [`studio.md`](studio.md),
> [`effects-library.md`](effects-library.md),
> [`inputs-and-automations.md`](inputs-and-automations.md) and
> [issue #359](https://github.com/o2csi/candeo/issues/359) (MCP).
>
> The goal: let an MCP-compatible agent create, validate and preview an effect,
> then propose applying it, without manual copy and paste. **No lighting change
> may result from generating or previewing alone.** No network service is turned
> on implicitly.

## 1. The architecture today — checked in the repository

```mermaid
flowchart TB
    subgraph Frontend["Tauri window / Vue 3"]
        Monaco["Monaco: editing, types, diagnostics"]
        TS["TypeScript source (.ts)"]
        Transpile["ts.transpileModule: TS → JS"]
        Sim["Vue simulator: draws the frames it receives"]
        Monaco --> TS --> Transpile
    end
    subgraph Backend["Candeo's Rust backend"]
        IPC["Typed Tauri IPC commands"]
        Store["Effect library: source + compiled cache"]
        Runtime["One QuickJS engine"]
        Preview["Preview loop: nothing sent to the hardware"]
        Applied["Applied loop: hardware output"]
        Store --> Runtime
        Runtime --> Preview
        Runtime --> Applied
    end
    Transpile -->|"save_effect_source / cache_effect"| IPC
    IPC --> Store
    Preview -->|"frames"| Sim
    Applied --> Native["Native HID device"]
    Ext["Local applications / Home Assistant"] --> Signal["Local HTTP signals API: opt-in, token, limited capabilities"]
    Signal --> Rules["Rule engine: cron / idle / signals"]
    Rules --> Runtime
```

**Checked in:** `apps/desktop/src/editor/effect.ts`, `apps/desktop/src/editor/library.ts`,
`apps/desktop/src/views/EditorView.vue`, `apps/desktop/src-tauri/src/storage.rs`,
`docs/api/commands.md`. The window compiles TypeScript sources with the compiler
Monaco ships. Preview and applied rendering use the same QuickJS runtime; only the
applied loop writes to the hardware.

The HTTP **signals** API exists already. It is **not** a general control API: the
sender supplies named values, and the person decides in the application which
rules they trigger.

## 2. The target architecture — to validate before implementing

```mermaid
flowchart TB
    Agent["Assistant / MCP client"]
    UI["Vue + Monaco / a future Composer"]
    Ext["Scripts / Home Assistant"]
    subgraph Adapters["Input adapters (no duplicated business logic)"]
        MCP["Local MCP (opt-in): tools + resources"]
        Tauri["Tauri IPC"]
        Signals["Existing HTTP signals (opt-in)"]
    end
    subgraph Domain["Rust domain services / source of truth"]
        Effects["Library + authoring + validation"]
        Automations["Automations + settings"]
        Devices["Device discovery and capabilities"]
    end
    subgraph Runtime["Execution"]
        Engine["QuickJS / frames"]
        Preview["Preview without hardware output"]
        HID["Native HID transport"]
        OpenRGB["OpenRGB adapter (future PoC, optional)"]
    end
    Agent --> MCP
    UI --> Tauri
    Ext --> Signals
    MCP --> Effects
    MCP --> Automations
    MCP --> Devices
    Tauri --> Effects
    Tauri --> Automations
    Tauri --> Devices
    Signals --> Automations
    Effects --> Engine
    Automations --> Engine
    Devices --> Engine
    Engine --> Preview
    Engine --> HID
    Engine -.-> OpenRGB
    Preview --> UI
    REST["General REST: backlog, not needed by MCP"]:::deferred
    REST -.-> Domain
    classDef deferred stroke-dasharray: 5 5,fill:#eee,color:#333
```

**Principle:** MCP, Tauri IPC and the HTTP signals API do not talk to each other;
each calls the shared domain services. **No general REST API** is needed by MCP.
A full REST API stays out of scope until someone needs it.

The services layer is a **refactoring target**, not a module that already exists
under that name.

## 3. Effects created by an agent: the proposed flow

```mermaid
sequenceDiagram
    actor User as Person
    participant Agent as MCP client / agent
    participant MCP as Local MCP server
    participant Domain as Effects domain
    participant Compiler as Transpilation + validation
    participant Engine as QuickJS / preview
    participant Hardware as Device
    User->>Agent: Describes the effect wanted
    Agent->>MCP: Reads the API version, layouts, examples
    MCP-->>Agent: Versioned resources
    Agent->>MCP: Validates TypeScript (without applying)
    MCP->>Compiler: Parse / transpile / check
    Compiler-->>MCP: Structured diagnostics
    MCP-->>Agent: Errors or runnable code
    loop Corrections when needed
        Agent->>MCP: Previews with a layout
        MCP->>Domain: Creates a trial context
        Domain->>Engine: Runs without HID output
        Engine-->>MCP: Frames / state / errors
        MCP-->>Agent: A rendering the agent can use
    end
    Agent->>MCP: Saves a user effect
    MCP->>Domain: Atomic, validated write to the library
    MCP-->>Agent: Id and confirmation
    Agent->>User: Proposes applying it on a device
    User->>Agent: Explicit confirmation
    Agent->>MCP: Applies the effect
    MCP->>Domain: Checks permissions, target and state
    Domain->>Engine: Starts the applied loop
    Engine->>Hardware: Sends the frames
```

The MCP tools are **proposed**, not implemented: `list_effects`, `get_layout`,
`validate_effect`, `preview_effect`, `save_effect`, `apply_effect`,
`list_automations`. The person's confirmation must be enforced by the product's
policy and by the server for sensitive operations, not rest on the agent's good
intentions alone.

### Documentation for agents

- Source of truth: `@candeo/effects-api`, its types, documentation, examples and
  the API version actually installed.
- Recommended **MCP resource**: `candeo://effects-api` (a summary of the
  documentation and its version), possibly with `candeo://examples`. No redundant
  `get_effect_api` tool is needed when the client reads resources.
- An optional, derived **SKILL.md** for agents that install skills: how to use it
  and the workflow, **not** a copy of the API schema that could drift.
- Capability metadata, so that an agent adapts its calls to the local version.

## 4. The TypeScript compiler — an open decision

**Today:** the TypeScript service shipped with Monaco produces the diagnostics and
runs `transpileModule` in the WebView. Monaco's plain syntax colouring does not
depend on the Rust backend having a compiler, but its semantic diagnostics,
completion and type tooling depend on the editor's TypeScript service.

**The likely need:** an MCP server reachable without an open window must not
depend on a call into a hidden WebView. A TS → JS transpilation on the Rust side
is to be studied. **Oxc** is a native candidate: it parses and transforms
TypeScript; it does not by itself replace TypeScript's full semantic checking.

To decide through a PoC (#364), with the shipped effects and adversarial cases:

1. Compare the JS emitted by Monaco's TypeScript and by Oxc: ESM format,
   `@candeo/effects-api` imports, syntax, and an ES2020 / QuickJS target.
2. Measure the differences in diagnostics, binary size, startup time, memory and
   dependency footprint.
3. Test syntax errors, type errors, malicious code, timeouts, an unknown API,
   unsaved sources.
4. Keep **one shared validation contract** for Tauri IPC and MCP: no silent
   divergence in behaviour.
5. Do not replace Monaco's compiler on principle; keep the editor and its
   TypeScript features.

**Result of the PoC (#364):** Oxc, under conditions. Every shipped and typed
effect renders the same frames through Oxc as through `transpileModule`, about
30 times faster, for about 2.3 MiB of binary. The conditions: settings for
TypeScript's class-field semantics, a size limit and a large stack because Oxc
can crash the process on deep nesting, one Rust validation contract shared by
IPC and MCP, and Rust 1.97. Types stay with the editor; TypeScript's checker
running in QuickJS is the measured option for an explicit check. Tables,
differences and what remains: [`headless-transpile-poc.md`](headless-transpile-poc.md).

## 5. Access, safety and scope

- MCP **off by default**, local only, needing neither the Internet nor an account.
- Choose `stdio` or Streamable HTTP according to the clients aimed at and Tauri's
  lifecycle; over HTTP, protect the endpoint with authentication and
  authorisation, and with Origin / DNS-rebinding checks. `127.0.0.1` alone is not
  authentication.
- Reading and previewing must neither touch the hardware nor change automations.
- Writes limited to user effects and the typed operations designed for them: no
  arbitrary file system access, no raw HID commands, no implicit upload of
  diagnostics or telemetry.
- Code from an agent is untrusted: keep QuickJS's sandbox, quotas and
  validations, path and size checks, a bounded local log.
- `save`, `preview` and `apply` kept apart. Ask for confirmation before any
  hardware action, before replacing an active effect, and before changing rules.
- **No general REST server in this work.**

## 6. Questions for the decision record

- MCP transport and host process (in the Tauri process, or a local sidecar).
- Headless transpilation: Oxc, another option, or a bounded reuse of the existing
  pipeline. Measured in [`headless-transpile-poc.md`](headless-transpile-poc.md):
  Oxc, under the conditions of its §6.
- Rendering preview frames as an image without duplicating the simulator's
  geometry.
- Output schemas, resource versioning and standard errors.
- The consent model for applied effects and changed automations.

**External references (inspiration, not an implementation to copy):**

- Hypercolor's [MCP](https://github.com/hyperb1iss/hypercolor/blob/main/docs/content/api/mcp.md)
  and [authoring](https://github.com/hyperb1iss/hypercolor/blob/main/crates/hypercolor-daemon/src/mcp/tools/authoring.rs):
  `install_effect` + `get_effect_console` + `capture_canvas`.
- Hypercolor runs **standalone HTML effects** (often built from TypeScript with
  Bun). **Candeo does not run HTML effects**: it runs JS transpiled from TS effects,
  in QuickJS.
