import { describe, expect, it } from 'vitest'

import { ASSISTANTS, shellQuoted, snippet, tomlString } from './assistants'

// Made-up paths: one Store alias, one AppImage, one Windows account with a quote.
const WINDOWS = 'C:\\Users\\someone\\AppData\\Local\\Microsoft\\WindowsApps\\candeo.exe'
const LINUX = "/home/someone/Apps/Candeo's.AppImage"
const QUOTED = "C:\\Users\\o'someone\\AppData\\Local\\Microsoft\\WindowsApps\\candeo.exe"

describe('snippet', () => {
  it('writes the files Claude Desktop and Cursor read, the path escaped as JSON', () => {
    for (const assistant of ['claudeDesktop', 'cursor'] as const) {
      const text = snippet(assistant, WINDOWS)
      expect(JSON.parse(text)).toEqual({
        mcpServers: { candeo: { command: WINDOWS, args: ['--mcp'] } },
      })
      expect(text).toContain('C:\\\\Users')
    }
  })

  it('writes VS Code’s mcp.json, with the server’s type', () => {
    expect(JSON.parse(snippet('vscode', LINUX))).toEqual({
      servers: { candeo: { type: 'stdio', command: LINUX, args: ['--mcp'] } },
    })
  })

  it('writes Copilot CLI’s mcp-config.json, with the tools it requires', () => {
    const text = snippet('copilotCli', WINDOWS)
    expect(JSON.parse(text)).toEqual({
      mcpServers: { candeo: { type: 'stdio', command: WINDOWS, args: ['--mcp'], tools: ['*'] } },
    })
    expect(text).toContain('C:\\\\Users')
  })

  it('writes a table of Codex’s config.toml, a Windows path as a literal string', () => {
    expect(snippet('codex', WINDOWS)).toBe(
      [
        '[mcp_servers.candeo]',
        "command = 'C:\\Users\\someone\\AppData\\Local\\Microsoft\\WindowsApps\\candeo.exe'",
        'args = ["--mcp"]',
      ].join('\n'),
    )
  })

  it('writes a path holding a single quote as a basic string for Codex', () => {
    expect(snippet('codex', QUOTED)).toContain(
      `command = "C:\\\\Users\\\\o'someone\\\\AppData\\\\Local\\\\Microsoft\\\\WindowsApps\\\\candeo.exe"`,
    )
    expect(snippet('codex', LINUX)).toContain(`command = "/home/someone/Apps/Candeo's.AppImage"`)
  })

  it('gives Claude Code a command for every project, the path quoted', () => {
    expect(snippet('claudeCode', WINDOWS)).toBe(
      `claude mcp add --transport stdio --scope user candeo -- "${WINDOWS}" --mcp`,
    )
  })

  it('gives any other assistant the command and its argument', () => {
    expect(snippet('other', LINUX)).toBe(`'/home/someone/Apps/Candeo'\\''s.AppImage' --mcp`)
  })

  it('has text for every assistant offered', () => {
    for (const assistant of ASSISTANTS) {
      expect(snippet(assistant, WINDOWS)).toContain('--mcp')
    }
  })
})

describe('shellQuoted', () => {
  it('quotes a Windows path in double quotes, a Unix one in single quotes', () => {
    expect(shellQuoted('C:\\Program Files\\Candeo\\candeo.exe')).toBe(
      '"C:\\Program Files\\Candeo\\candeo.exe"',
    )
    expect(shellQuoted('/usr/bin/candeo')).toBe("'/usr/bin/candeo'")
    expect(shellQuoted('/opt/$HOME/candeo')).toBe("'/opt/$HOME/candeo'")
  })
})

describe('tomlString', () => {
  it('keeps backslashes, spaces, double quotes and tabs as they are in a literal string', () => {
    expect(tomlString('C:\\Program Files\\Candeo\\candeo.exe')).toBe(
      "'C:\\Program Files\\Candeo\\candeo.exe'",
    )
    expect(tomlString('/opt/"candeo"\t/candeo')).toBe(`'/opt/"candeo"\t/candeo'`)
  })

  it('escapes a string a literal one cannot hold', () => {
    expect(tomlString(`/opt/it's "here"\\candeo`)).toBe(`"/opt/it's \\"here\\"\\\\candeo"`)
    expect(tomlString('/opt/new\nline\u007f\u0001')).toBe('"/opt/new\\nline\\u007F\\u0001"')
  })
})
