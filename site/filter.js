// A filter over each list of devices on the page: the supported ones, and the
// definitions to verify, whose tables share one box.
//
// It appears only once a list stops fitting in a glance: below `LEAST` rows,
// reading them is faster than typing, and a search box over three lines is
// furniture. Like the copy buttons, it is built here rather than written into
// the page, so a browser without JavaScript is never shown a control that
// filters nothing.
const LEAST = 4

/** Lowercase and without accents, so "sourís" finds "souris". */
const plain = (text) =>
  text
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '')
    .toLowerCase()

document.querySelectorAll('section').forEach((section, index) => {
  const tables = [...section.querySelectorAll('table.devices')]
  const rows = tables.flatMap((table) => [...table.tBodies[0].rows])
  if (rows.length < LEAST) return

  // What the count names: devices by default, definitions where the section says so.
  const noun = section.dataset.noun ?? 'devices'
  const searched = rows.map((row) => ({ row, text: plain(row.textContent) }))

  const box = document.createElement('div')
  box.className = 'filter'
  box.innerHTML = `<input type="search" id="filter-${index}" autocomplete="off"
      placeholder="Filter by maker, model, ids…" aria-label="Filter the ${noun}"
      aria-describedby="count-${index}" />
    <p class="note" id="count-${index}" aria-live="polite"></p>`
  // Above the first table, or above the heading that names it.
  const first = tables[0].closest('.scroller')
  const heading = first.previousElementSibling
  ;(heading?.tagName === 'H3' ? heading : first).before(box)

  const field = box.querySelector('input')
  const count = box.querySelector('p')

  function filter() {
    const query = plain(field.value.trim())
    let shown = 0
    for (const { row, text } of searched) {
      const matches = text.includes(query)
      row.hidden = !matches
      shown += matches ? 1 : 0
    }
    // A table left with no row goes, with the heading naming it.
    for (const table of tables) {
      const scroller = table.closest('.scroller')
      const empty = [...table.tBodies[0].rows].every((row) => row.hidden)
      scroller.hidden = empty
      const heading = scroller.previousElementSibling
      if (heading?.tagName === 'H3') heading.hidden = empty
    }
    const all = `${rows.length} ${noun}`
    count.textContent = query === '' ? all : shown === 0 ? `No ${noun.replace(/s$/, '')} matches` : `${shown} of ${all}`
  }

  field.addEventListener('input', filter)
  field.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      field.value = ''
      filter()
    }
  })
  filter()
})
