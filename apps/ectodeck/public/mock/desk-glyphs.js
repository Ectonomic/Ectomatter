// Ectodeck's glyphs for desk peripherals: one drawing style for every kind
// (a 64 unit square, 2.5 unit strokes, rounded corners and joins; keys solid,
// as keyboards are drawn), each device drawn from the view it is known by,
// top-down where it can be, reduced to what makes it that device.
window.DESK_GLYPHS = (() => {
  const S = 'fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"';
  const key = (x, y, w = 4, h = 4) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="1" fill="currentColor"/>`;
  const svg = inner => `<svg viewBox="0 0 64 64" ${S}>${inner}</svg>`;

  // A keyboard as keyboards are drawn: a thin outline, and solid keys laid out
  // in four staggered rows as on a real board. Each row fills the same width;
  // widths are in key units (Tab, Caps Lock, Enter and the spacebar wider).
  const solid = (x, y, w, h) => `<rect x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${w.toFixed(2)}" height="${h.toFixed(2)}" rx="0.9" fill="currentColor" stroke="none"/>`;
  const rows = [[1, 1, 1, 1, 1, 1, 1, 1, 1, 1], [1.5, 1, 1, 1, 1, 1, 1, 1, 1.5], [1.9, 1, 1, 1, 1, 1, 1, 2.1], [1, 1, 1, 5, 1, 1, 1, 1]];
  const KX = 7.5, KW = 49, KH = 3.3, GAP = 1.25, ROW = 4.9, KY = 23.3;
  let kb = `<rect x="3.5" y="19" width="57" height="26" rx="3.2"/>`;
  rows.forEach((units, r) => {
    const unit = (KW - GAP * (units.length - 1)) / units.reduce((a, b) => a + b, 0);
    let x = KX;
    for (const u of units) { kb += solid(x, KY + r * ROW, u * unit, KH); x += u * unit + GAP; }
  });

  // A macro pad to match: solid keys in a grid of four by three, three dials.
  let pad = `<rect x="6" y="14" width="52" height="36" rx="4"/>`;
  for (let r = 0; r < 3; r++) for (let c = 0; c < 4; c++) pad += solid(10.5 + c * 8.2, 19.5 + r * 8.7, 6.4, 6.4);
  pad += `<circle cx="49.5" cy="22.7" r="2.7"/><circle cx="49.5" cy="31.4" r="2.7"/><circle cx="49.5" cy="40.1" r="2.7"/>`;

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
