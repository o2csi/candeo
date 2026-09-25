// A copy button on the blocks someone is meant to run or paste: commands and
// code. Commands written inside a sentence keep none: a button between two
// words is noise.
//
// The buttons are built here rather than written into the page, so a browser
// without a clipboard never shows a button that does nothing. The text is read
// before the button is inserted: appending it to the block itself would make
// "Copy" part of what the block says.
const icons = {
  mark: '<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
}

/** One icon, drawn with the button's own colour. */
function drawn(name) {
  return (
    `<svg class="${name}" viewBox="0 0 24 24" fill="none" stroke="currentColor" ` +
    `stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">` +
    `${icons[name]}</svg>`
  )
}

/** Gives one block a button that copies what it says. */
function copyable(block) {
  const text = block.textContent.trim()
  if (!text) {
    return
  }

  const wrapper = document.createElement('div')
  wrapper.className = 'copyable'
  block.replaceWith(wrapper)
  wrapper.append(block)

  const button = document.createElement('button')
  button.type = 'button'
  button.className = 'copy'
  button.title = 'Copy'
  // The name is a live region: the button stays in place and only its label
  // changes, which a screen reader would otherwise announce on focus alone.
  button.innerHTML = `${drawn('mark')}${drawn('check')}<span class="said" aria-live="polite">Copy</span>`
  const said = button.querySelector('.said')
  let settle

  button.addEventListener('click', async () => {
    // One pending reset at a time: a click during the last one's wait would
    // otherwise leave the check mark up while the button says something else.
    clearTimeout(settle)
    let copied = true
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      // Refused or unavailable: say so, rather than claim a copy that never happened.
      copied = false
    }
    button.classList.toggle('done', copied)
    button.title = said.textContent = copied ? 'Copied' : 'Copy failed'
    settle = setTimeout(() => {
      button.classList.remove('done')
      button.title = said.textContent = 'Copy'
    }, 1800)
  })

  wrapper.append(button)
}

if (navigator.clipboard) {
  for (const block of document.querySelectorAll('pre, .command')) {
    copyable(block)
  }
}
