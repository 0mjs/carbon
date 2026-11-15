// A new tab is the Carbon dashboard. Ask first whether Carbon is answering, so a stopped Carbon gives a plain
// note instead of a browser error page.
const CARBON = 'http://localhost:3000/'
fetch(CARBON, { mode: 'no-cors', signal: AbortSignal.timeout(1200) }).then(
  () => location.replace(CARBON),
  () => (document.getElementById('down').hidden = false),
)
