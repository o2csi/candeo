import ts from 'typescript'
import { describe, expect, it } from 'vitest'

/**
 * The options Monaco's TypeScript mode formats with (`FormatHelper` in
 * `monaco-editor/esm/vs/languages/features/typescript/languageFeatures.js`,
 * 0.56), for the editor's two-space indentation.
 */
const MONACO: ts.FormatCodeOptions = {
  ConvertTabsToSpaces: true,
  TabSize: 2,
  IndentSize: 2,
  IndentStyle: ts.IndentStyle.Smart,
  NewLineCharacter: '\n',
  InsertSpaceAfterCommaDelimiter: true,
  InsertSpaceAfterSemicolonInForStatements: true,
  InsertSpaceBeforeAndAfterBinaryOperators: true,
  InsertSpaceAfterKeywordsInControlFlowStatements: true,
  InsertSpaceAfterFunctionKeywordForAnonymousFunctions: true,
  InsertSpaceAfterOpeningAndBeforeClosingNonemptyParenthesis: false,
  InsertSpaceAfterOpeningAndBeforeClosingNonemptyBrackets: false,
  InsertSpaceAfterOpeningAndBeforeClosingTemplateStringBraces: false,
  PlaceOpenBraceOnNewLineForControlBlocks: false,
  PlaceOpenBraceOnNewLineForFunctions: false,
}

function formatted(text: string): string {
  const host: ts.LanguageServiceHost = {
    getScriptFileNames: () => ['effect.ts'],
    getScriptVersion: () => '1',
    getScriptSnapshot: (name) =>
      name === 'effect.ts' ? ts.ScriptSnapshot.fromString(text) : undefined,
    getCurrentDirectory: () => '',
    getCompilationSettings: () => ({}),
    getDefaultLibFileName: () => 'lib.d.ts',
    fileExists: (name) => name === 'effect.ts',
    readFile: () => undefined,
  }
  const edits = ts.createLanguageService(host).getFormattingEditsForDocument('effect.ts', MONACO)
  return [...edits]
    .sort((a, b) => b.span.start - a.span.start)
    .reduce((t, e) => t.slice(0, e.span.start) + e.newText + t.slice(e.span.start + e.span.length), text)
}

/** The effects shipped with the application, as authors write them, by path. */
const SHIPPED = import.meta.glob<string>('../../../../packages/effects/*.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
})

describe('Ctrl+S on a shipped effect', () => {
  // A duplicated shipped effect saved at once must not come back reshaped.
  it.each(Object.entries(SHIPPED))('leaves %s as it is', (_, source) => {
    const text = source.replace(/\r\n/g, '\n')
    expect(formatted(text)).toBe(text)
  })
})
