import { describe, expect, it } from 'vitest'

import { ASSISTANTS, shellQuoted, snippet } from './assistants'

// Made-up paths: one Store alias, one AppImage.
const WINDOWS = 'C:\\Users\\someone\\AppData\\Local\\Microsoft\\WindowsApps\\candeo.exe'
const LINUX = "/home/someone/Apps/Candeo's.AppImage"

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
