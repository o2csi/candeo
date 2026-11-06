//! The MCP server one connection talks to (`docs/design/mcp-server.md` §4, §6):
//! the tools and the resources, over rmcp.
//!
//! Generic over a [`Host`], which runs the reads where the domain lives: the
//! main thread for Candeo, the test itself for the tests. So the whole protocol
//! side — schemas, errors, the bounds, the resources' URIs — is tested through
//! rmcp's own client, with no window and no device.
//!
//! Tools are listed and dispatched by hand rather than through rmcp's macros:
//! an argument of the wrong shape must come back as a tool error the model can
//! read and correct, never as a protocol error it does not see.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
    Implementation, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, MetaObject,
    PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ResourceTemplate, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::views::{
    AutomationsView, CheckView, DevicesView, EffectSourceView, EffectsApiView, EffectsView,
    LayoutView, PresetsView, SignalsView,
};
use super::Connections;
use crate::failure::Failure;
use crate::DeviceRef;

/// What the domain answers. Each read is what one window command reads.
pub(crate) trait Reads {
    fn devices(&self) -> Result<DevicesView, Failure>;
    /// `None`: the default layout, which a preview borrows without a device.
    fn layout(&self, device: Option<DeviceRef>) -> Result<LayoutView, Failure>;
    fn effects(&self) -> Result<EffectsView, Failure>;
    fn effect(&self, id: &str) -> Result<EffectSourceView, Failure>;
    fn presets(&self, effect: Option<&str>) -> Result<PresetsView, Failure>;
    fn automations(&self) -> Result<AutomationsView, Failure>;
    fn check_automation(&self, rule: &Value) -> Result<CheckView, Failure>;
    fn signals(&self) -> Result<SignalsView, Failure>;
}

/// Where the reads run.
pub(crate) trait Host: Send + Sync + 'static {
    type Reads: Reads;
    fn with<T, F>(&self, read: F) -> impl Future<Output = Result<T, Failure>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Self::Reads) -> Result<T, Failure> + Send + 'static;
}

/// Calls a connection may make in a second (§4.1): the specification asks
/// servers to rate-limit, and no reading loop needs more.
pub(crate) const CALLS_PER_SECOND: usize = 20;

/// The longest message a connection may send: a source of 1 MiB fits four
/// times over, escaped as JSON (§4.1).
pub(crate) const MAX_MESSAGE: usize = 4 * 1024 * 1024;

/// How long the effects API's resources stay fresh: they change only with
/// Candeo. Everything else is read again each time.
const API_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// The installed effects API, its guide and its starting point: the files
/// themselves, embedded, never a copy that could drift (§6).
pub(crate) const API_DECLARATION: &str =
    include_str!("../../../../../packages/effects-api/src/index.ts");
pub(crate) const API_GUIDE: &str = include_str!("../../../../../packages/effects-api/src/guide.md");
const API_EXAMPLE: &str = include_str!("../../../../../packages/effects-api/src/example.ts");

const API_URI: &str = "candeo://effects-api";
const GUIDE_URI: &str = "candeo://effects-api/guide";
const EXAMPLE_URI: &str = "candeo://effects-api/example";
const STATE_URI: &str = "candeo://state";
const EFFECT_TEMPLATE: &str = "candeo://effects/{source}/{name}";
const LAYOUT_TEMPLATE: &str = "candeo://devices/{device}/layout";
const DEFAULT_LAYOUT_URI: &str = "candeo://devices/default/layout";

/// The example as a person's effect writes it: inside the package it imports
/// `./index`, which the engine does not resolve (`editor/template.ts`).
fn example() -> String {
    API_EXAMPLE.replace("'./index'", "'@candeo/effects-api'")
}

fn candeo_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The longest assistant name kept: it is a label it chooses, shown and logged.
const MAX_NAME: usize = 64;

/// The name an assistant gives, made safe to show and log: printable, short.
pub(crate) fn assistant_name(given: &str) -> String {
    given
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_owned()
}

// ---------------------------------------------------------------- inputs

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct NoInput {}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct LayoutInput {
    /// The device whose layout to give; absent, the default layout, which
    /// effects are written and previewed against without a device.
    #[serde(default)]
    device: Option<DeviceRef>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EffectsInput {
    /// Only the effects shipped with Candeo, or only the person's.
    #[serde(default)]
    kind: Option<KindFilter>,
}

#[derive(Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum KindFilter {
    Builtin,
    User,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EffectInput {
    /// The effect's id, as `list_effects` gives it: `shipped:Rain`, `user:Rain`.
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PresetsInput {
    /// Only this effect's presets, by its id.
    #[serde(default)]
    effect: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CheckInput {
    /// The rule to check, as `list_automations` gives rules.
    #[schemars(with = "crate::automations::resolver::Rule")]
    rule: Value,
}

/// A refusal, as the model reads it (§4.1): the `Failure` code
/// (`errors.<code>` in the catalogs), its parameters, and its English text.
#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct ToolError {
    pub code: String,
    pub params: std::collections::BTreeMap<String, String>,
    pub message: String,
}

impl From<&Failure> for ToolError {
    fn from(failure: &Failure) -> Self {
        Self {
            code: failure.code.to_owned(),
            params: failure
                .params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            message: failure.to_string(),
        }
    }
}

/// A tool error. Its JSON goes as text only: a client checking structured
/// content against the tool's output schema would find an error's shape wrong.
fn error_result(failure: &Failure) -> CallToolResult {
    let json = serde_json::to_string(&ToolError::from(failure)).unwrap_or_default();
    CallToolResult::error(vec![ContentBlock::text(json)])
}

fn structured(value: Value) -> CallToolResult {
    CallToolResult::structured(value)
}

fn arguments<T: for<'de> Deserialize<'de>>(
    arguments: &Option<rmcp::model::JsonObject>,
) -> Result<T, Failure> {
    let value = Value::Object(arguments.clone().unwrap_or_default());
    serde_json::from_value(value).map_err(|e| Failure::new("mcpInvalidArguments").with("detail", e))
}

// ---------------------------------------------------------------- tools

/// One tool: its name, what the model reads of it, its input and output
/// schemas. Every tool of this phase reads, and reaches nothing beyond the
/// computer (§3.4).
fn tool<I: JsonSchema + 'static, O: JsonSchema + 'static>(
    name: &'static str,
    title: &'static str,
    description: &'static str,
) -> Tool {
    Tool::new(name, description, Arc::new(rmcp::model::JsonObject::new()))
        .with_title(title)
        .with_input_schema::<I>()
        .with_output_schema::<O>()
        .annotate(
            ToolAnnotations::with_title(title)
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        )
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        tool::<NoInput, DevicesView>(
            "list_devices",
            "List devices",
            "The lighting devices Candeo knows, plugged in or not: whether each is controlled, \
             open and split into regions, what effect runs on it or on each region with its \
             settings, the rule interrupting it, and whether automations are paused.",
        ),
        tool::<LayoutInput, LayoutView>(
            "get_layout",
            "Get a layout",
            "A device's lights: each light's index, matrix row and column, key name and \
             rectangle in key units, its regions and outline. Without a device, the default \
             layout effects are written against. An effect receives these lights as `layout.keys`.",
        ),
        tool::<EffectsInput, EffectsView>(
            "list_effects",
            "List effects",
            "The effects in the library: id, state, what each declares (description, settings, \
             inputs, the lights it suits, what it requires) and a sample of its colours.",
        ),
        tool::<EffectInput, EffectSourceView>(
            "get_effect",
            "Get an effect",
            "One effect from the library, with its TypeScript source. The shipped effects are \
             examples of the effects API.",
        ),
        tool::<NoInput, EffectsApiView>(
            "get_effects_api",
            "Get the effects API",
            "The installed effects API's declaration (`@candeo/effects-api`, every type and \
             helper with its documentation) and the guide to writing an effect, as embedded \
             resources, with the API's version and Candeo's. Read it before writing an effect.",
        ),
        tool::<PresetsInput, PresetsView>(
            "list_presets",
            "List presets",
            "Named settings saved for an effect, offered on every device and region.",
        ),
        tool::<NoInput, AutomationsView>(
            "list_automations",
            "List automations",
            "The automation rules in priority order: when each applies, on which devices, what \
             it shows and for how long; why a rule does nothing; which devices each interrupts \
             now; and whether automations are paused.",
        ),
        tool::<CheckInput, CheckView>(
            "check_automation",
            "Check an automation",
            "Whether a rule would be accepted, without saving anything: why not, its next three \
             starts for a cron rule, and the devices it names that Candeo does not know.",
        ),
        tool::<NoInput, SignalsView>(
            "list_signals",
            "List signals",
            "Whether Candeo receives signals from other software, and the signals held now, by \
             name, with the time each has left. Their values are never given.",
        ),
    ]
}

// ---------------------------------------------------------------- the bounds

/// At most [`CALLS_PER_SECOND`] calls in any second.
#[derive(Default)]
pub(crate) struct RateLimit {
    recent: VecDeque<Instant>,
}

impl RateLimit {
    pub(crate) fn allow(&mut self, now: Instant) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|&at| now.duration_since(at) >= Duration::from_secs(1))
        {
            self.recent.pop_front();
        }
        if self.recent.len() >= CALLS_PER_SECOND {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

// ---------------------------------------------------------------- the server

pub(crate) struct Server<H: Host> {
    host: Arc<H>,
    connections: Arc<Connections>,
    connection: u64,
    limit: Mutex<RateLimit>,
}

impl<H: Host> Server<H> {
    pub(crate) fn new(host: Arc<H>, connections: Arc<Connections>, connection: u64) -> Self {
        Self {
            host,
            connections,
            connection,
            limit: Mutex::new(RateLimit::default()),
        }
    }

    /// The assistant's name, as it sends it with every request (2026-07-28) or
    /// once at `initialize` (2025-11-25): a label, never an authorisation.
    fn learn_name(&self, context: &RequestContext<RoleServer>) {
        if let Some(info) = context.client_info() {
            self.connections
                .name(self.connection, &assistant_name(&info.name));
        }
    }

    fn allowed(&self) -> bool {
        self.limit.lock().unwrap().allow(Instant::now())
    }

    async fn read<T, F>(&self, read: F) -> Result<Value, Failure>
    where
        T: Serialize + Send + 'static,
        F: FnOnce(&H::Reads) -> Result<T, Failure> + Send + 'static,
    {
        let value = self.host.with(read).await?;
        serde_json::to_value(value).map_err(Failure::unexpected)
    }

    async fn call(&self, request: &CallToolRequestParams) -> Result<CallToolResult, Failure> {
        let args = &request.arguments;
        let value = match request.name.as_ref() {
            "list_devices" => {
                arguments::<NoInput>(args)?;
                self.read(|r| r.devices()).await?
            }
            "get_layout" => {
                let input: LayoutInput = arguments(args)?;
                self.read(move |r| r.layout(input.device)).await?
            }
            "list_effects" => {
                let input: EffectsInput = arguments(args)?;
                self.read(move |r| {
                    let mut view = r.effects()?;
                    if let Some(kind) = input.kind {
                        view.effects.retain(|e| {
                            (kind == KindFilter::User) == (e.kind == super::views::KindView::User)
                        });
                    }
                    Ok(view)
                })
                .await?
            }
            "get_effect" => {
                let input: EffectInput = arguments(args)?;
                self.read(move |r| r.effect(&input.id)).await?
            }
            "get_effects_api" => {
                arguments::<NoInput>(args)?;
                return Ok(effects_api_result());
            }
            "list_presets" => {
                let input: PresetsInput = arguments(args)?;
                self.read(move |r| r.presets(input.effect.as_deref()))
                    .await?
            }
            "list_automations" => {
                arguments::<NoInput>(args)?;
                self.read(|r| r.automations()).await?
            }
            "check_automation" => {
                let input: CheckInput = arguments(args)?;
                self.read(move |r| r.check_automation(&input.rule)).await?
            }
            "list_signals" => {
                arguments::<NoInput>(args)?;
                self.read(|r| r.signals()).await?
            }
            // Checked by the caller before coming here.
            other => return Err(Failure::unexpected(format!("no tool {other}"))),
        };
        Ok(structured(value))
    }

    async fn resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        let text = |uri: &str, mime: &str, text: String| {
            ResourceContents::text(text, uri).with_mime_type(mime)
        };
        let api_meta = || {
            let mut meta = MetaObject::new();
            meta.0.insert(
                "apiVersion".into(),
                crate::storage::EFFECTS_API_VERSION.into(),
            );
            meta.0
                .insert("candeoVersion".into(), candeo_version().into());
            meta
        };
        let fresh = |contents: ResourceContents| {
            ReadResourceResult::new(vec![contents])
                .with_ttl_ms(0)
                .with_cache_scope(CacheScope::Private)
        };
        let api = |contents: ResourceContents| {
            ReadResourceResult::new(vec![contents.with_meta(api_meta())])
                .with_ttl_ms(API_TTL_MS)
                .with_cache_scope(CacheScope::Private)
        };
        let json = |value: Result<Value, Failure>| {
            value
                .map(|v| fresh(text(uri, "application/json", v.to_string())))
                .map_err(|f| resource_error(uri, &f))
        };
        match Uri::parse(uri) {
            Some(Uri::Api) => Ok(api(text(uri, "text/x-typescript", API_DECLARATION.into()))),
            Some(Uri::Guide) => Ok(api(text(uri, "text/markdown", API_GUIDE.into()))),
            Some(Uri::Example) => Ok(api(text(uri, "text/x-typescript", example()))),
            Some(Uri::State) => json(
                self.read(|r| Ok(super::views::state_view(r.devices()?)))
                    .await,
            ),
            Some(Uri::Layout(device)) => json(self.read(move |r| r.layout(device)).await),
            Some(Uri::Effect(id)) => {
                let read = self.read(move |r| r.effect(&id).map(|e| e.source)).await;
                match read {
                    Ok(Value::String(source)) => Ok(fresh(text(uri, "text/x-typescript", source))),
                    Ok(_) => Err(ErrorData::internal_error("source is not text", None)),
                    Err(f) => Err(resource_error(uri, &f)),
                }
            }
            None => Err(ErrorData::resource_not_found(
                format!("no resource {uri}"),
                None,
            )),
        }
    }
}

/// A resource that cannot be read: not found when it names nothing, otherwise
/// the refusal as a tool would give it.
fn resource_error(uri: &str, failure: &Failure) -> ErrorData {
    let data = serde_json::to_value(ToolError::from(failure)).ok();
    match failure.code {
        "effectNotFound" | "noLayout" | "deviceNotFound" => {
            ErrorData::resource_not_found(format!("no resource {uri}"), data)
        }
        _ => ErrorData::internal_error(failure.to_string(), data),
    }
}

fn effects_api_result() -> CallToolResult {
    let view = EffectsApiView {
        api_version: crate::storage::EFFECTS_API_VERSION,
        candeo_version: candeo_version().to_owned(),
        resources: vec![API_URI.into(), GUIDE_URI.into()],
    };
    let value = serde_json::to_value(&view).unwrap_or_default();
    let mut result = CallToolResult::structured(value);
    for (uri, mime, text) in [
        (API_URI, "text/x-typescript", API_DECLARATION),
        (GUIDE_URI, "text/markdown", API_GUIDE),
    ] {
        result.content.push(ContentBlock::resource(
            ResourceContents::text(text, uri).with_mime_type(mime),
        ));
    }
    result
}

/// A resource's URI, read.
#[derive(Debug, PartialEq)]
enum Uri {
    Api,
    Guide,
    Example,
    State,
    /// `None`: the default layout.
    Layout(Option<DeviceRef>),
    /// An effect's id, `user:Rain`.
    Effect(String),
}

impl Uri {
    fn parse(uri: &str) -> Option<Self> {
        match uri {
            API_URI => return Some(Self::Api),
            GUIDE_URI => return Some(Self::Guide),
            EXAMPLE_URI => return Some(Self::Example),
            STATE_URI => return Some(Self::State),
            DEFAULT_LAYOUT_URI => return Some(Self::Layout(None)),
            _ => {}
        }
        if let Some(rest) = uri.strip_prefix("candeo://effects/") {
            let (source, name) = rest.split_once('/')?;
            if !matches!(source, "shipped" | "user") {
                return None;
            }
            let name = decode(name)?;
            return (!name.is_empty()).then(|| Self::Effect(format!("{source}:{name}")));
        }
        let device = uri
            .strip_prefix("candeo://devices/")?
            .strip_suffix("/layout")?;
        let (vid, pid) = device.split_once('-')?;
        let hex = |s: &str| {
            (s.len() == 4)
                .then(|| u16::from_str_radix(s, 16).ok())
                .flatten()
        };
        Some(Self::Layout(Some(DeviceRef {
            vid: hex(vid)?,
            pid: hex(pid)?,
        })))
    }
}

fn effect_uri(id: &str) -> Option<String> {
    let (source, name) = id.split_once(':')?;
    Some(format!("candeo://effects/{source}/{}", crate::query(name)))
}

fn layout_uri(device: DeviceRef) -> String {
    format!(
        "candeo://devices/{:04x}-{:04x}/layout",
        device.vid, device.pid
    )
}

/// Percent-decoding, of what [`crate::query`] encodes.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl<H: Host> ServerHandler for Server<H> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("candeo", candeo_version())
                .with_title("Candeo")
                .with_website_url(env!("CARGO_PKG_REPOSITORY")),
        )
        .with_instructions(
            "Candeo drives keyboard and device lighting on this computer. These tools read \
             the devices, their layouts, the effects library, presets, automation rules and \
             the installed effects API; none of them changes anything. Effects are TypeScript \
             modules written against `@candeo/effects-api`: read `candeo://effects-api/guide` \
             first.",
        )
    }

    /// This server implements 2026-07-28, and answers clients still on an
    /// earlier revision through the `initialize` handshake. A later revision is
    /// claimed once implemented.
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(ProtocolVersion::known_up_to(&ProtocolVersion::V_2026_07_28))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.learn_name(&context);
        Ok(ListToolsResult::with_all_items(tools())
            .with_ttl_ms(API_TTL_MS)
            .with_cache_scope(CacheScope::Private))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.learn_name(&context);
        // An unknown tool is the one protocol error (§4.1).
        if !tools().iter().any(|t| t.name == request.name) {
            return Err(ErrorData::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            ));
        }
        if !self.allowed() {
            let failure = Failure::new("mcpTooManyCalls").with("max", CALLS_PER_SECOND);
            return Ok(error_result(&failure).into());
        }
        tracing::debug!(connection = self.connection, tool = %request.name, "assistant read");
        let result = match self.call(&request).await {
            Ok(result) => result,
            Err(failure) => error_result(&failure),
        };
        Ok(result.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.learn_name(&context);
        // Listing reads the library and enumerates the devices: bounded as a call.
        if !self.allowed() {
            let failure = Failure::new("mcpTooManyCalls").with("max", CALLS_PER_SECOND);
            return Err(ErrorData::internal_error(failure.to_string(), None));
        }
        let listed = self
            .host
            .with(|r| Ok((r.effects()?, r.devices()?)))
            .await
            .map_err(|f| ErrorData::internal_error(f.to_string(), None))?;
        let (effects, devices) = listed;
        let mut resources = vec![
            Resource::new(API_URI, "effects-api")
                .with_title("The effects API")
                .with_description("Every type and helper of `@candeo/effects-api`, as installed.")
                .with_mime_type("text/x-typescript"),
            Resource::new(GUIDE_URI, "effects-api-guide")
                .with_title("Writing an effect")
                .with_description("How to write, check and save an effect for this version.")
                .with_mime_type("text/markdown"),
            Resource::new(EXAMPLE_URI, "effects-api-example")
                .with_title("A first effect")
                .with_description("The editor's starting point.")
                .with_mime_type("text/x-typescript"),
            Resource::new(STATE_URI, "state")
                .with_title("What runs")
                .with_description(
                    "What runs on each device and region, the rules interrupting them, the pause.",
                )
                .with_mime_type("application/json"),
            Resource::new(DEFAULT_LAYOUT_URI, "default-layout")
                .with_title("The default layout")
                .with_mime_type("application/json"),
        ];
        resources.extend(effects.effects.iter().filter_map(|e| {
            Some(
                Resource::new(effect_uri(&e.id)?, e.id.clone())
                    .with_title(e.name.clone())
                    .with_mime_type("text/x-typescript"),
            )
        }));
        let mut seen = Vec::new();
        for device in &devices.devices {
            if !seen.contains(&device.device) {
                seen.push(device.device);
                resources.push(
                    Resource::new(
                        layout_uri(device.device),
                        format!("layout-{}", device.device),
                    )
                    .with_title(format!("{}'s layout", device.name))
                    .with_mime_type("application/json"),
                );
            }
        }
        Ok(ListResourcesResult::with_all_items(resources)
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        self.learn_name(&context);
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(EFFECT_TEMPLATE, "effect")
                .with_title("An effect's source")
                .with_description("`source` is `shipped` or `user`, as in the effect's id; the name is percent-encoded.")
                .with_mime_type("text/x-typescript"),
            ResourceTemplate::new(LAYOUT_TEMPLATE, "layout")
                .with_title("A device's layout")
                .with_description("`device` is its VID and PID, four hexadecimal digits each: `1532-0292`.")
                .with_mime_type("application/json"),
        ])
        .with_ttl_ms(API_TTL_MS)
        .with_cache_scope(CacheScope::Private))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.learn_name(&context);
        if !self.allowed() {
            let failure = Failure::new("mcpTooManyCalls").with("max", CALLS_PER_SECOND);
            return Err(ErrorData::internal_error(
                failure.to_string(),
                serde_json::to_value(ToolError::from(&failure)).ok(),
            ));
        }
        tracing::debug!(connection = self.connection, "assistant read a resource");
        self.resource(&request.uri).await.map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::views::{
        DecisionView, DeviceView, EffectStateView, EffectView, KindView, LayoutView,
    };
    use rmcp::model::{ClientCapabilities, ClientConfig, ProtocolVersion};
    use rmcp::{ClientHandler, ClientLifecycleMode, ClientServiceExt, ServiceExt};

    const KEYBOARD: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };

    /// Made-up answers: the reads' own logic is tested in `views`; here, what
    /// crosses the protocol.
    struct Fixture;

    fn effect(id: &str, kind: KindView) -> EffectView {
        EffectView {
            id: id.into(),
            name: id.split_once(':').unwrap().1.into(),
            kind,
            state: EffectStateView::Ready,
            error: None,
            description: Value::String("Drops falling".into()),
            params: serde_json::Map::new(),
            api_version: 1,
            inputs: Vec::new(),
            kinds: vec!["keys".into()],
            requires: Vec::new(),
            game: None,
            swatch: vec!["#0044ff".into()],
        }
    }

    impl Reads for Fixture {
        fn devices(&self) -> Result<DevicesView, Failure> {
            Ok(DevicesView {
                devices: vec![DeviceView {
                    device: KEYBOARD,
                    name: "DeathStalker V2 Pro".into(),
                    present: true,
                    state: DecisionView::Adopted,
                    open: true,
                    unverified: false,
                    firmware: Some("v1.5".into()),
                    lights: "keys".into(),
                    light_count: 106,
                    capabilities: vec!["matrix".into(), "geometry".into()],
                    split: false,
                    regions: Vec::new(),
                    runs: None,
                    region_runs: Vec::new(),
                    interruption: None,
                    reaching_keyboard: false,
                    open_error: None,
                    warnings: Vec::new(),
                }],
                automations_paused: false,
            })
        }
        fn layout(&self, device: Option<DeviceRef>) -> Result<LayoutView, Failure> {
            match device {
                Some(d) if d != KEYBOARD => Err(Failure::new("noLayout").with("device", d)),
                _ => Ok(LayoutView {
                    name: "Fixture".into(),
                    rows: 1,
                    cols: 2,
                    frame_len: 2,
                    lights: "keys".into(),
                    capabilities: vec!["geometry".into()],
                    keys: Vec::new(),
                    regions: Vec::new(),
                    outline: Vec::new(),
                }),
            }
        }
        fn effects(&self) -> Result<EffectsView, Failure> {
            Ok(EffectsView {
                effects: vec![
                    effect("shipped:Rain", KindView::Builtin),
                    effect("user:Starry night", KindView::User),
                ],
            })
        }
        fn effect(&self, id: &str) -> Result<EffectSourceView, Failure> {
            let effect = self
                .effects()?
                .effects
                .into_iter()
                .find(|e| e.id == id)
                .ok_or_else(|| Failure::new("effectNotFound").with("name", id))?;
            Ok(EffectSourceView {
                source: format!("// {}", effect.name),
                effect,
            })
        }
        fn presets(&self, _effect: Option<&str>) -> Result<PresetsView, Failure> {
            Ok(PresetsView {
                presets: Vec::new(),
            })
        }
        fn automations(&self) -> Result<AutomationsView, Failure> {
            Ok(AutomationsView {
                paused: true,
                rules: Vec::new(),
            })
        }
        fn check_automation(&self, rule: &Value) -> Result<CheckView, Failure> {
            Ok(super::super::views::check_view(
                rule,
                &[KEYBOARD],
                &chrono::Utc::now(),
            ))
        }
        fn signals(&self) -> Result<SignalsView, Failure> {
            Ok(SignalsView {
                receiving: false,
                signals: Vec::new(),
            })
        }
    }

    impl Host for Fixture {
        type Reads = Fixture;
        async fn with<T, F>(&self, read: F) -> Result<T, Failure>
        where
            T: Send + 'static,
            F: FnOnce(&Self::Reads) -> Result<T, Failure> + Send + 'static,
        {
            read(self)
        }
    }

    #[derive(Clone)]
    struct Assistant(ProtocolVersion);

    impl ClientHandler for Assistant {
        fn get_info(&self) -> ClientConfig {
            let mut info = ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new("test-assistant\u{7}", "1.0"),
            );
            info.protocol_version = self.0.clone();
            info
        }
    }

    /// A server on one end of an in-memory stream, and a client on the other,
    /// speaking `version`.
    async fn connect(
        version: ProtocolVersion,
    ) -> (
        rmcp::service::RunningService<rmcp::RoleClient, Assistant>,
        Arc<Connections>,
    ) {
        let connections = Arc::new(Connections::default());
        let id = connections.open();
        let server = Server::new(Arc::new(Fixture), Arc::clone(&connections), id);
        let (server_end, client_end) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            if let Ok(running) = server.serve(server_end).await {
                let _ = running.waiting().await;
            }
        });
        let lifecycle = if version.as_str() >= ProtocolVersion::V_2026_07_28.as_str() {
            ClientLifecycleMode::Discover {
                preferred_versions: vec![version.clone()],
            }
        } else {
            ClientLifecycleMode::Initialize
        };
        let client = Assistant(version)
            .serve_with_lifecycle(client_end, lifecycle)
            .await
            .expect("the client connects");
        (client, connections)
    }

    fn call(name: &'static str, arguments: Value) -> CallToolRequestParams {
        let mut params = CallToolRequestParams::new(name);
        params.arguments = arguments.as_object().cloned();
        params
    }

    fn text(result: &CallToolResult) -> String {
        match &result.content[0] {
            ContentBlock::Text(t) => t.text.clone(),
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn every_tool_reads_and_has_both_schemas() {
        let tools = tools();
        assert_eq!(tools.len(), 9);
        for tool in &tools {
            let annotations = tool.annotations.as_ref().unwrap();
            assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
            assert_eq!(annotations.destructive_hint, Some(false));
            assert_eq!(annotations.open_world_hint, Some(false));
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            assert!(tool.output_schema.is_some(), "{}", tool.name);
            // Names as the specification allows them, and as §4.1 writes them.
            assert!(tool
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    /// Both revisions reach the same tools over the same stream, and each tool
    /// answers on the fixtures.
    #[tokio::test]
    async fn each_tool_answers_in_both_revisions() {
        for version in [ProtocolVersion::V_2026_07_28, ProtocolVersion::V_2025_11_25] {
            let (client, connections) = connect(version.clone()).await;
            let listed = client.list_all_tools().await.unwrap();
            assert_eq!(listed.len(), 9, "{version}");

            let devices = client
                .call_tool(call("list_devices", serde_json::json!({})))
                .await
                .unwrap();
            assert_eq!(devices.is_error, Some(false));
            let structured = devices.structured_content.clone().unwrap();
            assert_eq!(structured["devices"][0]["device"]["vid"], 0x1532);
            // Also as text, for clients that read no structured content.
            assert_eq!(
                serde_json::from_str::<Value>(&text(&devices)).unwrap(),
                structured
            );

            let layout = client
                .call_tool(call(
                    "get_layout",
                    serde_json::json!({ "device": { "vid": 0x1532, "pid": 0x0292 } }),
                ))
                .await
                .unwrap();
            assert_eq!(layout.structured_content.unwrap()["cols"], 2);

            let users = client
                .call_tool(call("list_effects", serde_json::json!({ "kind": "user" })))
                .await
                .unwrap();
            let users = users.structured_content.unwrap();
            assert_eq!(users["effects"].as_array().unwrap().len(), 1);
            assert_eq!(users["effects"][0]["id"], "user:Starry night");

            let rain = client
                .call_tool(call(
                    "get_effect",
                    serde_json::json!({ "id": "shipped:Rain" }),
                ))
                .await
                .unwrap();
            assert_eq!(rain.structured_content.unwrap()["source"], "// Rain");

            for name in ["list_presets", "list_automations", "list_signals"] {
                let result = client
                    .call_tool(call(name, serde_json::json!({})))
                    .await
                    .unwrap();
                assert_eq!(result.is_error, Some(false), "{name}");
            }

            let check = client
                .call_tool(call(
                    "check_automation",
                    serde_json::json!({ "rule": { "id": "x", "when": { "kind": "idle", "minutes": 0 } } }),
                ))
                .await
                .unwrap();
            assert_eq!(check.structured_content.unwrap()["ok"], false);

            // The assistant's name, cleaned of what cannot be shown.
            assert_eq!(connections.names(), ["test-assistant"]);
            client.cancel().await.unwrap();
        }
    }

    #[tokio::test]
    async fn the_api_comes_embedded_with_its_versions() {
        let (client, _) = connect(ProtocolVersion::V_2026_07_28).await;
        let api = client
            .call_tool(call("get_effects_api", serde_json::json!({})))
            .await
            .unwrap();
        let structured = api.structured_content.clone().unwrap();
        assert_eq!(structured["apiVersion"], 1);
        assert_eq!(structured["candeoVersion"], env!("CARGO_PKG_VERSION"));
        let embedded: Vec<&str> = api
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Resource(r) => match &r.resource {
                    ResourceContents::TextResourceContents { uri, .. } => Some(uri.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(embedded, [API_URI, GUIDE_URI]);
        client.cancel().await.unwrap();
    }

    /// A refusal is a tool result the model reads: its code, its parameters, its
    /// English text. Only an unknown tool is a protocol error.
    #[tokio::test]
    async fn refusals_are_tool_errors_with_their_code() {
        let (client, _) = connect(ProtocolVersion::V_2026_07_28).await;
        let missing = client
            .call_tool(call(
                "get_effect",
                serde_json::json!({ "id": "user:Nothing" }),
            ))
            .await
            .unwrap();
        assert_eq!(missing.is_error, Some(true));
        assert!(missing.structured_content.is_none());
        let error: Value = serde_json::from_str(&text(&missing)).unwrap();
        assert_eq!(error["code"], "effectNotFound");
        assert_eq!(error["params"]["name"], "user:Nothing");
        assert_eq!(error["message"], "No effect is named “user:Nothing”.");

        let wrong = client
            .call_tool(call(
                "get_layout",
                serde_json::json!({ "device": "the keyboard" }),
            ))
            .await
            .unwrap();
        assert_eq!(wrong.is_error, Some(true));
        let error: Value = serde_json::from_str(&text(&wrong)).unwrap();
        assert_eq!(error["code"], "mcpInvalidArguments");

        let unknown = client
            .call_tool(call("apply_effect", serde_json::json!({})))
            .await;
        assert!(unknown.is_err());
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn every_resource_reads() {
        let (client, _) = connect(ProtocolVersion::V_2026_07_28).await;
        let listed = client.list_all_resources().await.unwrap();
        let uris: Vec<&str> = listed.iter().map(|r| r.uri.as_str()).collect();
        for uri in [
            API_URI,
            GUIDE_URI,
            EXAMPLE_URI,
            STATE_URI,
            DEFAULT_LAYOUT_URI,
            "candeo://effects/shipped/Rain",
            "candeo://effects/user/Starry%20night",
            "candeo://devices/1532-0292/layout",
        ] {
            assert!(uris.contains(&uri), "{uri} not in {uris:?}");
            let read = client
                .read_resource(ReadResourceRequestParams::new(uri))
                .await
                .unwrap_or_else(|e| panic!("{uri}: {e}"));
            assert_eq!(read.cache_scope, Some(CacheScope::Private), "{uri}");
            assert!(!read.contents.is_empty(), "{uri}");
        }
        let example = client
            .read_resource(ReadResourceRequestParams::new(EXAMPLE_URI))
            .await
            .unwrap();
        let ResourceContents::TextResourceContents { text, meta, .. } = &example.contents[0] else {
            panic!("not text");
        };
        assert!(text.contains("from '@candeo/effects-api'"));
        assert_eq!(meta.as_ref().unwrap().0["apiVersion"], 1);

        let templates = client.list_all_resource_templates().await.unwrap();
        assert_eq!(templates.len(), 2);

        for uri in [
            "candeo://effects/user/Nothing",
            "candeo://devices/0001-0002/layout",
            "candeo://nothing",
        ] {
            assert!(
                client
                    .read_resource(ReadResourceRequestParams::new(uri))
                    .await
                    .is_err(),
                "{uri}"
            );
        }
        client.cancel().await.unwrap();
    }

    #[test]
    fn uris_read_back_what_they_name() {
        assert_eq!(
            Uri::parse("candeo://devices/1532-0292/layout"),
            Some(Uri::Layout(Some(KEYBOARD)))
        );
        assert_eq!(layout_uri(KEYBOARD), "candeo://devices/1532-0292/layout");
        let uri = effect_uri("user:Pluie d'été").unwrap();
        assert_eq!(
            Uri::parse(&uri),
            Some(Uri::Effect("user:Pluie d'été".into()))
        );
        assert_eq!(Uri::parse("candeo://effects/elsewhere/Rain"), None);
        assert_eq!(Uri::parse("candeo://devices/1532-292/layout"), None);
        assert_eq!(Uri::parse("candeo://effects/user/%zz"), None);
    }

    #[tokio::test]
    async fn calls_beyond_twenty_a_second_are_refused() {
        let (client, _) = connect(ProtocolVersion::V_2026_07_28).await;
        let mut refused = 0;
        for _ in 0..(CALLS_PER_SECOND + 5) {
            let result = client
                .call_tool(call("list_signals", serde_json::json!({})))
                .await
                .unwrap();
            if result.is_error == Some(true) {
                let error: Value = serde_json::from_str(&text(&result)).unwrap();
                assert_eq!(error["code"], "mcpTooManyCalls");
                refused += 1;
            }
        }
        assert!(refused >= 5, "{refused}");
        client.cancel().await.unwrap();
    }

    #[test]
    fn the_rate_limit_is_a_sliding_second() {
        let mut limit = RateLimit::default();
        let start = Instant::now();
        for _ in 0..CALLS_PER_SECOND {
            assert!(limit.allow(start));
        }
        assert!(!limit.allow(start + Duration::from_millis(999)));
        assert!(limit.allow(start + Duration::from_secs(1)));
    }

    /// What the log keeps of a session, at the most verbose level and with the
    /// MCP library held as the application holds it: the connection, the
    /// assistant and the tool, never a source, an argument or a value.
    #[tokio::test]
    async fn the_log_holds_no_source_argument_or_value() {
        use tracing_subscriber::layer::SubscriberExt;

        #[derive(Clone, Default)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Capture {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl tracing_subscriber::fmt::MakeWriter<'_> for Capture {
            type Writer = Self;
            fn make_writer(&self) -> Self {
                self.clone()
            }
        }

        // The process's own subscriber, as the application has one: a
        // subscriber set on this thread alone misses events whenever another
        // test met the same log statements first, `tracing` caching their
        // interest for every thread. No other test installs a global one.
        static CAPTURE: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();
        let capture = CAPTURE
            .get_or_init(|| {
                let capture = Capture::default();
                let filter = tracing_subscriber::EnvFilter::new(crate::journal::directives(
                    crate::journal::LogLevel::Trace,
                ));
                let subscriber = tracing_subscriber::registry().with(filter).with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(capture.clone()),
                );
                tracing::subscriber::set_global_default(subscriber)
                    .expect("no other test installs a global subscriber");
                capture
            })
            .clone();

        let (client, _) = connect(ProtocolVersion::V_2026_07_28).await;
        client
            .call_tool(call(
                "get_effect",
                serde_json::json!({ "id": "shipped:Rain" }),
            ))
            .await
            .unwrap();
        client
            .call_tool(call(
                "check_automation",
                serde_json::json!({ "rule": { "id": "x", "when": { "kind": "signal", "name": "door", "equals": "open-sesame" } } }),
            ))
            .await
            .unwrap();
        client
            .read_resource(ReadResourceRequestParams::new(
                "candeo://effects/user/Starry%20night",
            ))
            .await
            .unwrap();
        client
            .call_tool(call(
                "get_layout",
                serde_json::json!({ "device": "a secret device" }),
            ))
            .await
            .unwrap();
        client.cancel().await.unwrap();

        // Other tests run meanwhile and log too: what MCP and its library wrote.
        let log: String = String::from_utf8_lossy(&capture.0.lock().unwrap())
            .lines()
            .filter(|line| line.contains("candeo_desktop_lib::mcp") || line.contains("rmcp"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(log.contains("assistant read"), "{log}");
        assert!(log.contains("tool=get_effect"), "{log}");
        assert!(log.contains("assistant=\"test-assistant\""), "{log}");
        for secret in [
            "// Rain",
            "// Starry night",
            "open-sesame",
            "a secret device",
            "Starry",
        ] {
            assert!(!log.contains(secret), "{secret} logged:\n{log}");
        }
    }

    #[test]
    fn an_assistant_name_is_shown_safely() {
        assert_eq!(assistant_name("  Claude Code\n"), "Claude Code");
        assert_eq!(assistant_name(&"x".repeat(200)).len(), MAX_NAME);
    }

    /// The guide names every tool it tells the model to use, and they exist.
    #[test]
    fn the_guide_names_existing_tools() {
        let names: Vec<String> = tools().iter().map(|t| t.name.to_string()).collect();
        for tool in [
            "get_effects_api",
            "get_layout",
            "list_effects",
            "get_effect",
        ] {
            assert!(names.iter().any(|n| n == tool));
            assert!(API_GUIDE.contains(tool), "{tool} not in the guide");
        }
    }
}
