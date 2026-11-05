import { defineEffect } from '@candeo/effects-api'

interface Cell {
  readonly row: number
  readonly col: number
}

type Grid = Cell[][]

const grid: Grid = []

// Line 15 throws on the second frame: where does each path say it is?
function cellAt(row: number, col: number): Cell {
  const line = grid[row]
  return line![col]!.row === row ? line![col]! : { row, col }
}

export default defineEffect({
  render({ frame, frameIndex }) {
    if (frameIndex > 0) cellAt(3, 4)
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
