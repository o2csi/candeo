// TypeScript's CommonJS export form, which an ES module cannot have.
export = {
  render({ frame }: { frame: { fill(c: { r: number; g: number; b: number }): void } }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
}
