/**
 * Runs a composable inside a component, without a DOM.
 *
 * Composables that register lifecycle hooks need a component instance. A
 * renderer whose nodes are plain objects gives them one, and unmounting it runs
 * `onBeforeUnmount` the way leaving a screen does.
 */

import { createRenderer } from 'vue'

interface Node {
  children: Node[]
  parent: Node | null
}

const node = (): Node => ({ children: [], parent: null })

const { createApp } = createRenderer<Node, Node>({
  createElement: node,
  createText: node,
  createComment: node,
  insert(child, parent) {
    child.parent = parent
    parent.children.push(child)
  },
  remove(child) {
    const siblings = child.parent?.children
    siblings?.splice(siblings.indexOf(child), 1)
    child.parent = null
  },
  parentNode: (n) => n.parent,
  nextSibling: () => null,
  setText() {},
  setElementText() {},
  patchProp() {},
})

export function withSetup<T>(composable: () => T): { result: T; unmount: () => void } {
  let result!: T
  const app = createApp({
    setup() {
      result = composable()
      return () => null
    },
  })
  app.mount(node())
  return { result, unmount: () => app.unmount() }
}
