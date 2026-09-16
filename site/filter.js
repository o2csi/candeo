// A filter over the devices table.
//
// It appears only once the list stops fitting in a glance: below `LEAST` rows,
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

const table = document.querySelector('table.devices')
const rows = table ? [...table.tBodies[0].rows] : []

if (rows.length >= LEAST) {
  const searched = rows.map((row) => ({ row, text: plain(row.textContent) }))

  const box = document.createElement('div')
  box.className = 'filter'
  box.innerHTML = `<input type="search" id="device-filter" autocomplete="off"
      placeholder="Filter by maker, model, kind…" aria-label="Filter the devices"
      aria-describedby="device-count" />
    <p class="note" id="device-count" aria-live="polite"></p>`
  table.closest('.scroller').before(box)

  const field = box.querySelector('input')
  const count = box.querySelector('#device-count')

  function filter() {
    const query = plain(field.value.trim())
    let shown = 0
    for (const { row, text } of searched) {
      const matches = text.includes(query)
      row.hidden = !matches
      shown += matches ? 1 : 0
    }
    const all = `${rows.length} devices`
    count.textContent = query === '' ? all : shown === 0 ? 'No device matches' : `${shown} of ${all}`
  }

  field.addEventListener('input', filter)
  field.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      field.value = ''
      filter()
    }
  })
  filter()
}
