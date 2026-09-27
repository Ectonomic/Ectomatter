// Ectodeck's glyphs for desk peripherals: one drawing style for every kind
// (a 64 unit square, 2.5 unit strokes, rounded corners and joins, all in
// line), each device drawn from the view it is known by,
// top-down where it can be, reduced to what makes it that device.
window.DESK_GLYPHS = (() => {
  const S = 'fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"';
  const key = (x, y, w = 4, h = 4) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="1" fill="currentColor"/>`;
  const svg = inner => `<svg viewBox="0 0 64 64" ${S}>${inner}</svg>`;

  // a full keyboard: two rows of keys and the spacebar, drawn in line like the
  // rest of the set, and the number pad set apart
  const row = (x0, x1, y) => `<path d="M${x0} ${y}H${x1}" stroke-dasharray="3.2 3.4"/>`;
  let kb = `<rect x="3" y="18" width="58" height="28" rx="4"/>`;
  kb += row(9.5, 40, 25.5) + row(9.5, 40, 32) + `<path d="M9.5 38.5h1M16 38.5h16M38 38.5h1"/>`;
  kb += row(47, 55, 25.5) + row(47, 55, 32) + row(47, 55, 38.5);

  // a macro pad: a grid of large keys with its dials beside it
  let pad = `<rect x="5" y="14" width="54" height="36" rx="5"/>`;
  for (let r = 0; r < 2; r++) for (let c = 0; c < 3; c++) pad += `<rect x="${11 + c * 11}" y="${20 + r * 12}" width="8" height="8" rx="2"/>`;
  pad += `<circle cx="50" cy="24" r="4.2"/><circle cx="50" cy="40" r="4.2"/>`;

  // a mouse from above: its two buttons, its wheel
  const mouse = `<path d="M32 6c-8.8 0-14 6.4-14 16v16c0 11 6.3 20 14 20s14-9 14-20V22c0-9.6-5.2-16-14-16z"/><path d="M32 6v17M18 23h28"/><rect x="29.6" y="11" width="4.8" height="8" rx="2.4" fill="currentColor" stroke="none"/>`;

  // a headset from the front: the band, the cups, the microphone
  const headset = `<path d="M13 36v-6a19 19 0 0 1 38 0v6"/><rect x="9" y="34" width="10" height="16" rx="4"/><rect x="45" y="34" width="10" height="16" rx="4"/><path d="M14 50c0 5 5 8 12 8h3"/>`;

  // a desk microphone on its stand
  const mic = `<rect x="24" y="6" width="16" height="28" rx="8"/><path d="M17 28a15 15 0 0 0 30 0M32 43v10M22 57h20"/>`;

  // a controller: its body, the d-pad, the face buttons
  const pad2 = `<path d="M18 19h28c7 0 11 6 12.5 15l1.4 9c.9 5.8-5.4 9.2-9.4 5.2L44 42H20l-6.5 6.2c-4 4-10.3.6-9.4-5.2l1.4-9C7 25 11 19 18 19z"/><path d="M20 26v10M15 31h10"/>${key(42, 26.5, 4, 4)}${key(47, 31.5, 4, 4)}`;

  // a speaker: the cabinet, a tweeter over a woofer
  const speaker = `<rect x="17" y="5" width="30" height="54" rx="5"/><circle cx="32" cy="18" r="4.5"/><circle cx="32" cy="40" r="10"/><circle cx="32" cy="40" r="3" fill="currentColor" stroke="none"/>`;

  // a webcam on its clip
  const webcam = `<circle cx="32" cy="26" r="17"/><circle cx="32" cy="26" r="7"/><path d="M24 43l-5 9h26l-5-9"/>`;

  // a display on its stand
  const display = `<rect x="4" y="9" width="56" height="35" rx="3.5"/><path d="M32 44v9M21 55h22"/>`;

  return {
    keyboard: svg(kb), macropad: svg(pad), mouse: svg(mouse), headset: svg(headset), microphone: svg(mic),
    controller: svg(pad2), speaker: svg(speaker), webcam: svg(webcam), display: svg(display),
  };
})();
