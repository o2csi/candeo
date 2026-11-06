// What Settings › Assistants hands the person to paste into an assistant
// (`docs/design/mcp-server.md` §8): the command that starts `candeo --mcp`,
// written the way each assistant's documentation asks for it.
//
// Pure, so that each format, and the quoting of a path with spaces or
// backslashes, is tested without Rust or an assistant.

/** The assistants Settings offers text for, in the order offered. */
export const ASSISTANTS = ['claudeDesktop', 'claudeCode', 'vscode', 'cursor', 'other'] as const

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

/**
 * The text to paste for `assistant`, starting `command`:
 *
 * - Claude Desktop, `claude_desktop_config.json`, and Cursor, `~/.cursor/mcp.json`:
 *   `mcpServers`;
 * - Claude Code: `claude mcp add`, for every project of the user;
 * - VS Code, `mcp.json`: `servers`, with its type;
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
    case 'claudeCode':
      return `claude mcp add --transport stdio --scope user ${NAME} -- ${shellQuoted(command)} ${ARGUMENT}`
    case 'other':
      return `${shellQuoted(command)} ${ARGUMENT}`
  }
}
