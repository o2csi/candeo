// What Settings › Assistants hands the person to paste into an assistant
// (`docs/design/mcp-server.md` §8): the command that starts `candeo --mcp`,
// written the way each assistant's documentation asks for it.
//
// Codex and GitHub Copilot CLI get the lines for their file rather than their
// `mcp add` command: their desktop apps and editor extensions read that file
// too, and may be installed without the command.
//
// Pure, so that each format, and the quoting of a path with spaces, backslashes
// or quotes, is tested without Rust or an assistant.

/** The assistants Settings offers text for, in the order offered. */
export const ASSISTANTS = [
  'claudeDesktop',
  'claudeCode',
  'codex',
  'copilotCli',
  'vscode',
  'cursor',
  'other',
] as const

export type Assistant = (typeof ASSISTANTS)[number]

/** The server's name in an assistant's configuration. */
const NAME = 'candeo'

/** The argument that makes Candeo's executable the bridge. */
const ARGUMENT = '--mcp'

/**
 * A path quoted for a terminal: double quotes for Windows' shells, which the
 * three of them read alike; single quotes elsewhere, where double quotes would
 * still expand `$`.
 */
export function shellQuoted(path: string): string {
  if (path.includes('\\') || /^[A-Za-z]:/.test(path)) return `"${path}"`
  return `'${path.split("'").join(`'\\''`)}'`
}

/** JSON as the assistants' own files are written: two spaces. */
function json(value: unknown): string {
  return JSON.stringify(value, null, 2)
}

/** What a TOML basic string writes with a backslash (TOML 1.0, *String*). */
const TOML_ESCAPES: Record<string, string> = {
  '"': '\\"',
  '\\': '\\\\',
  '\b': '\\b',
  '\t': '\\t',
  '\n': '\\n',
  '\f': '\\f',
  '\r': '\\r',
}

/** U+0000 to U+001F and U+007F: of them, TOML strings hold only tab as it is. */
function control(char: string): boolean {
  const code = char.charCodeAt(0)
  return code < 0x20 || code === 0x7f
}

/** `char` as a TOML basic string writes it. */
function basic(char: string): string {
  const escape = TOML_ESCAPES[char]
  if (escape) return escape
  if (!control(char)) return char
  return `\\u${char.charCodeAt(0).toString(16).toUpperCase().padStart(4, '0')}`
}

/**
 * A string as TOML writes it: literal, in single quotes, so that a Windows path
 * keeps its backslashes as they are. A literal string cannot hold a single
 * quote, nor a control character other than tab: such a path is a basic string,
 * escaped.
 */
export function tomlString(value: string): string {
  const chars = [...value]
  if (!chars.some((char) => char === "'" || (control(char) && char !== '\t'))) {
    return `'${value}'`
  }
  return `"${chars.map(basic).join('')}"`
}

/**
 * The text to paste for `assistant`, starting `command`:
 *
 * - Claude Desktop, `claude_desktop_config.json`, and Cursor, `~/.cursor/mcp.json`:
 *   `mcpServers`;
 * - Claude Code: `claude mcp add`, for every project of the user;
 * - Codex, `~/.codex/config.toml`: a `[mcp_servers.candeo]` table;
 * - GitHub Copilot CLI, `~/.copilot/mcp-config.json`: `mcpServers`, with the
 *   tools it enables, which that file requires;
 * - VS Code and Visual Studio, `mcp.json` and `.mcp.json`: `servers`, with its
 *   type;
 * - another assistant: the command and its argument.
 */
export function snippet(assistant: Assistant, command: string): string {
  const server = { command, args: [ARGUMENT] }
  switch (assistant) {
    case 'claudeDesktop':
    case 'cursor':
      return json({ mcpServers: { [NAME]: server } })
    case 'vscode':
      return json({ servers: { [NAME]: { type: 'stdio', ...server } } })
    case 'copilotCli':
      return json({ mcpServers: { [NAME]: { type: 'stdio', ...server, tools: ['*'] } } })
    case 'codex':
      return [
        `[mcp_servers.${NAME}]`,
        `command = ${tomlString(command)}`,
        `args = ${JSON.stringify([ARGUMENT])}`,
      ].join('\n')
    case 'claudeCode':
      return `claude mcp add --transport stdio --scope user ${NAME} -- ${shellQuoted(command)} ${ARGUMENT}`
    case 'other':
      return `${shellQuoted(command)} ${ARGUMENT}`
  }
}
