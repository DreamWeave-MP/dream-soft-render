// dream-soft-render's hero: the header's golden frame, drawn again, call by call, the way the crate
// draws it, on a wall of 80x60 physical pixels.
//
// The drawing is real. Below is a JavaScript port of the crate's rules (docs/rules.md): pixels are
// tested at their centres, triangle edges use the crate's edge function and top-left tie-break,
// rectangles cover min < centre <= max, colours are premultiplied, and blending is the crate's
// source-over with its rounding. The scene is the golden "window" frame (the header image, 640x480)
// modelled as the draws egui hands the crate, at one eighth of its size: a clear, rectangles, fans
// for every rounded shape, glyph rows as textured rectangles, and the window's shadow and feathered
// edges as triangles whose vertex colours differ.
//
// Each draw takes the path the crate's rasterizer would take for it (docs/performance.md): a
// rectangle, a fan or a one-colour triangle fills whole spans, row by row; glyphs copy texel rows;
// gradients and feathering are covered and coloured pixel by pixel, after a coverage scan finds
// where each row starts. The wall shows the difference: spans land a row at a time, per-pixel work
// sparkles cell by cell. A glass shard shows the draw being rasterized, a bar marks the current
// row, and the bezel reads out the call, its path, the pixels it wrote, and an FNV-1a hash of the
// surface. When the next frame's bytes match, the frame is skipped, as render_egui skips it, and
// nothing moves. Each full pass ends by presenting the crate's own output: the golden PNG.
//
// The page's picture is the still: it shows until the first frame, stays without WebGL, and the
// wall takes its place when live. Colours come from the site's CSS tokens. Nothing runs while the
// hero is off screen or the tab is hidden; resolution drops if frames run slow; under
// prefers-reduced-motion one frame is drawn.

import * as THREE from './vendor/three.module.min.js';

const GRID_W = 80;
const GRID_H = 60;
const CELLS = GRID_W * GRID_H;
const UNIT = 8; // golden pixels per cell
const TARGET_ASPECT = 4 / 3;
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
const smooth = (t) => t * t * (3 - 2 * t);

function cssColor(name, fallback) {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const color = new THREE.Color();
  try {
    color.setStyle(value || fallback);
  } catch {
    color.setStyle(fallback);
  }
  return color;
}

function cssValue(name, fallback) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

// The crate's rules ------------------------------------------------------------------------------

function edge(a, b, c) {
  return (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x);
}

function edgeIsTopLeft(a, b) {
  return a.y < b.y || (a.y === b.y && a.x > b.x);
}

function edgeIncludesBoundary(a, b, area) {
  return area < 0 ? edgeIsTopLeft(a, b) : !edgeIsTopLeft(a, b);
}

function edgeCoversPixel(weight, includesBoundary) {
  return weight > 0 || (weight === 0 && includesBoundary);
}

// Premultiplied source-over onto an opaque destination; the alpha byte is written as 255.
function blend(pixels, index, r, g, b, a) {
  if (a === 0) return;
  if (a === 255) {
    pixels[index] = r;
    pixels[index + 1] = g;
    pixels[index + 2] = b;
    pixels[index + 3] = 255;
    return;
  }
  const keep = 255 - a;
  pixels[index] = Math.min(255, r + Math.round(pixels[index] * keep / 255));
  pixels[index + 1] = Math.min(255, g + Math.round(pixels[index + 1] * keep / 255));
  pixels[index + 2] = Math.min(255, b + Math.round(pixels[index + 2] * keep / 255));
  pixels[index + 3] = 255;
}

// Straight colour to the crate's premultiplied Color: per channel (channel x alpha + 127) / 255.
function premultiply(r, g, b, alpha = 255) {
  const p = (channel) => Math.floor((channel * alpha + 127) / 255);
  return [p(r), p(g), p(b), alpha];
}

// Rectangles cover the pixels whose centres satisfy min < centre <= max on both axes.
function rectRange(min, max, size) {
  const first = Math.floor(min - 0.5) + 1;
  const last = Math.floor(max - 0.5);
  return [Math.max(0, first), Math.min(size - 1, last)];
}

// Which pixels a triangle covers, row by row: the crate's coverage test at each pixel centre,
// with its barycentric weights for interpolating vertex colours.
function triangleCoverage(v0, v1, v2, visit) {
  const area = edge(v0, v1, v2);
  if (!(area !== 0) || !Number.isFinite(area)) return;
  const inverse = 1 / area;
  const include0 = edgeIncludesBoundary(v1, v2, area);
  const include1 = edgeIncludesBoundary(v2, v0, area);
  const include2 = edgeIncludesBoundary(v0, v1, area);
  const minX = Math.max(0, Math.floor(Math.min(v0.x, v1.x, v2.x) - 0.5));
  const maxX = Math.min(GRID_W - 1, Math.ceil(Math.max(v0.x, v1.x, v2.x) - 0.5));
  const minY = Math.max(0, Math.floor(Math.min(v0.y, v1.y, v2.y) - 0.5));
  const maxY = Math.min(GRID_H - 1, Math.ceil(Math.max(v0.y, v1.y, v2.y) - 0.5));
  const centre = { x: 0, y: 0 };
  for (let y = minY; y <= maxY; y++) {
    centre.y = y + 0.5;
    for (let x = minX; x <= maxX; x++) {
      centre.x = x + 0.5;
      const w0 = edge(v1, v2, centre) * inverse;
      const w1 = edge(v2, v0, centre) * inverse;
      const w2 = edge(v0, v1, centre) * inverse;
      const covered = edgeCoversPixel(w0, include0) && edgeCoversPixel(w1, include1) && edgeCoversPixel(w2, include2);
      visit(x, y, covered, w0, w1, w2);
    }
  }
}

function fnv1a(bytes) {
  let hash = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    hash ^= bytes[i];
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

// The golden scene as egui draws it -------------------------------------------------------------

// The window frame's widgets, measured from the header image (golden pixels).
const TEXT = [190, 190, 190];
const TITLE_TEXT = [210, 210, 210];
const MUTED_TEXT = [150, 150, 150];
const PANEL = [27, 27, 27];
const FIELD = [10, 10, 10];
const WIDGET = [60, 60, 60];
const TITLE_BAR = [45, 45, 45];
const HANDLE = [180, 180, 180];
const FILL = [0, 92, 128];

export const GOLDEN_STATE = Object.freeze({ radio: 2, progress: 0.42, slider: 0.6, windowOpen: true, checks: [true, false, false], focus: -1, saving: false, booting: false });

// Labels and where they sit; drawn into the glyph atlas and rasterized as rows of texels.
function labels(state) {
  const list = [
    { text: 'Dream INI Importer', x: 8, y: 12, size: 19, color: TITLE_TEXT, box: [8, 10, 172, 30] },
    { text: 'Import Morrowind.ini settings into an OpenMW configuration.', x: 8, y: 33, size: 13, color: MUTED_TEXT, box: [8, 31, 372, 47] },
    { text: 'Morrowind.ini', x: 8, y: 61, size: 13, color: MUTED_TEXT, box: [8, 59, 94, 77] },
    { text: '/stora', x: 100, y: 61, size: 13, color: TEXT, box: [98, 59, 138, 77] },
    { text: 'Browse...', x: 150, y: 61, size: 13, color: TEXT, box: [147, 59, 208, 77] },
    { text: 'openmw.cfg', x: 8, y: 83, size: 13, color: MUTED_TEXT, box: [8, 81, 94, 99] },
    { text: '/stora', x: 100, y: 83, size: 13, color: TEXT, box: [98, 81, 138, 99] },
    { text: 'Import game files', x: 33, y: 119, size: 13, color: TEXT, box: [32, 117, 138, 133] },
    { text: 'Import fonts', x: 33, y: 140, size: 13, color: TEXT, box: [32, 138, 108, 154] },
    { text: 'Skip archives', x: 33, y: 161, size: 13, color: TEXT, box: [32, 159, 110, 175] },
    { text: 'win1252', x: 19, y: 181, size: 13, color: TEXT, box: [18, 179, 72, 195] },
    { text: 'Encoding', x: 123, y: 181, size: 13, color: MUTED_TEXT, box: [122, 179, 180, 195] },
    { text: 'Central European', x: 33, y: 203, size: 13, color: TEXT, box: [32, 201, 136, 217] },
    { text: 'Preview', x: 12, y: 236, size: 13, color: TEXT, box: [10, 234, 60, 251] },
    { text: 'Save As...', x: 73, y: 236, size: 13, color: state.saving ? [255, 255, 255] : TEXT, box: [72, 234, 132, 251] },
    { text: 'Update', x: 148, y: 236, size: 13, color: TEXT, box: [145, 234, 198, 251] },
    { text: '▸ Warnings (2)', x: 12, y: 266, size: 13, color: TEXT, box: [10, 264, 102, 281] },
    { text: 'Controller: A select · B back · X preview · Y save · Start update · Select quit', x: 8, y: 287, size: 13, color: MUTED_TEXT, box: [8, 285, 432, 300] },
  ];
  if (state.windowOpen) {
    list.push(
      { text: 'Choose encoding', x: 254, y: 89, size: 17, color: TEXT, box: [252, 87, 394, 108], window: true },
      { text: 'The INI text encoding decides how names are decoded.', x: 157, y: 122, size: 13, color: TEXT, box: [156, 120, 478, 136], window: true },
      { text: 'win1250', x: 175, y: 141, size: 13, color: TEXT, box: [174, 139, 230, 155], window: true },
      { text: 'win1251', x: 175, y: 162, size: 13, color: TEXT, box: [174, 160, 230, 176], window: true },
      { text: 'win1252', x: 175, y: 183, size: 13, color: TEXT, box: [174, 181, 230, 197], window: true },
      { text: state.slider.toFixed(2), x: 272, y: 204, size: 13, color: TEXT, box: [266, 202, 304, 219], window: true },
      { text: 'Preview scale', x: 313, y: 204, size: 13, color: TEXT, box: [312, 202, 394, 219], window: true },
      { text: `${Math.round(state.progress * 100)}%`, x: 165, y: 225, size: 13, color: [230, 240, 245], box: [163, 223, 196, 240], window: true },
      { text: 'OK', x: 162, y: 246, size: 13, color: TEXT, box: [159, 244, 182, 262], window: true },
      { text: 'Cancel', x: 195, y: 246, size: 13, color: TEXT, box: [193, 244, 236, 262], window: true },
    );
  }
  return list;
}

// The glyph atlas: every label drawn at golden size, then reduced to the wall's resolution, as
// premultiplied RGBA. A label's glyphs are textured rectangles sampling it.
function glyphAtlas(state, fontFamily) {
  const canvas = document.createElement('canvas');
  canvas.width = GRID_W * UNIT;
  canvas.height = GRID_H * UNIT;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  context.textBaseline = 'top';
  for (const label of labels(state)) {
    context.font = `${label.size}px ${fontFamily}`;
    context.fillStyle = `rgb(${label.color.join(',')})`;
    context.fillText(label.text, label.x, label.y);
  }
  const source = context.getImageData(0, 0, canvas.width, canvas.height).data;
  const atlas = new Uint8Array(CELLS * 4);
  for (let y = 0; y < GRID_H; y++) {
    for (let x = 0; x < GRID_W; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < UNIT; sy++) {
        for (let sx = 0; sx < UNIT; sx++) {
          const i = ((y * UNIT + sy) * canvas.width + x * UNIT + sx) * 4;
          const alpha = source[i + 3] / 255;
          r += source[i] * alpha;
          g += source[i + 1] * alpha;
          b += source[i + 2] * alpha;
          a += source[i + 3];
        }
      }
      const n = UNIT * UNIT;
      // Coverage below a quarter is anti-aliasing fringe; at an eighth of the size it reads as dirt.
      const alpha = Math.round(a / n);
      const keep = alpha < 40 ? 0 : Math.min(255, Math.round(alpha * 1.6));
      const scale = alpha > 0 ? keep / alpha : 0;
      const o = (y * GRID_W + x) * 4;
      atlas[o] = Math.min(keep, Math.round(r / n * scale));
      atlas[o + 1] = Math.min(keep, Math.round(g / n * scale));
      atlas[o + 2] = Math.min(keep, Math.round(b / n * scale));
      atlas[o + 3] = keep;
    }
  }
  return atlas;
}

// Shapes, in golden pixels.
// Corners in the order top-left, top-right, bottom-right, bottom-left. A square corner is its
// corner point repeated, so every outline of a shape has the same number of points; the repeats
// make zero-area triangles, which cover nothing.
function roundedOutline(x0, y0, x1, y1, radius, corners = [true, true, true, true], segments = 4) {
  const points = [];
  const arc = (cx, cy, start, round, cornerX, cornerY) => {
    for (let i = 0; i <= segments; i++) {
      const angle = start + (i / segments) * (Math.PI / 2);
      points.push(round ? { x: cx + Math.cos(angle) * radius, y: cy + Math.sin(angle) * radius } : { x: cornerX, y: cornerY });
    }
  };
  arc(x1 - radius, y0 + radius, -Math.PI / 2, corners[1], x1, y0);
  arc(x1 - radius, y1 - radius, 0, corners[2], x1, y1);
  arc(x0 + radius, y1 - radius, Math.PI / 2, corners[3], x0, y1);
  arc(x0 + radius, y0 + radius, Math.PI, corners[0], x0, y0);
  return points;
}

function circleOutline(cx, cy, radius, segments = 12) {
  const points = [];
  for (let i = 0; i < segments; i++) {
    const angle = (i / segments) * Math.PI * 2;
    points.push({ x: cx + Math.cos(angle) * radius, y: cy + Math.sin(angle) * radius });
  }
  return points;
}

const toCells = (point) => ({ x: point.x / UNIT, y: point.y / UNIT });

// A draw call as the rasterizer meets it: its triangles (in cells), their vertex colours, and the
// path it takes.
function fan(name, outline, color) {
  const cells = outline.map(toCells);
  const centre = cells.reduce((sum, p) => ({ x: sum.x + p.x / cells.length, y: sum.y + p.y / cells.length }), { x: 0, y: 0 });
  const triangles = [];
  for (let i = 0; i < cells.length; i++) triangles.push([centre, cells[i], cells[(i + 1) % cells.length]]);
  return { name, kind: 'fan', path: 'spans', label: 'fan → spans', triangles, color: premultiply(...color), edges: 'outline' };
}

function rect(name, x0, y0, x1, y1, color, alpha = 255) {
  const a = toCells({ x: x0, y: y0 });
  const b = toCells({ x: x1, y: y1 });
  return { name, kind: 'rect', path: 'spans', label: 'rectangle → spans', box: [a.x, a.y, b.x, b.y], triangles: [[a, { x: b.x, y: a.y }, b], [a, b, { x: a.x, y: b.y }]], color: premultiply(...color, alpha), edges: 'outline' };
}

function triangle(name, points, color) {
  return { name, kind: 'triangle', path: 'spans', label: 'one-colour triangle → spans', triangles: [points.map(toCells)], color: premultiply(...color), edges: 'all' };
}

function text(name, box) {
  const a = toCells({ x: box[0], y: box[1] });
  const b = toCells({ x: box[2], y: box[3] });
  return { name, kind: 'text', path: 'texels', label: 'glyph quads → texel rows', box: [a.x, a.y, b.x, b.y], triangles: [[a, { x: b.x, y: a.y }, b], [a, b, { x: a.x, y: b.y }]], edges: 'outline' };
}

// A ring between two outlines with its own colour at each: a gradient, drawn pixel by pixel.
function ring(name, inner, outer, innerColor, outerColor, label) {
  const i = inner.map(toCells);
  const o = outer.map(toCells);
  const triangles = [];
  const colors = [];
  for (let k = 0; k < i.length; k++) {
    const n = (k + 1) % i.length;
    triangles.push([i[k], o[k], o[n]], [i[k], o[n], i[n]]);
    colors.push([innerColor, outerColor, outerColor], [innerColor, outerColor, innerColor]);
  }
  return { name, kind: 'gradient', path: 'pixels', label, triangles, colors, edges: 'all' };
}

export function drawList(state) {
  const list = [];
  list.push({ name: 'clear', kind: 'clear', path: 'overwrite', label: 'clear → overwrite', color: premultiply(...PANEL), triangles: [[{ x: 0, y: 0 }, { x: GRID_W, y: 0 }, { x: GRID_W, y: GRID_H }], [{ x: 0, y: 0 }, { x: GRID_W, y: GRID_H }, { x: 0, y: GRID_H }]], edges: 'outline' });
  const glyphs = (name, index) => {
    const label = labels(state).filter((item) => !item.window)[index];
    return text(name, label.box);
  };
  list.push(glyphs('heading', 0), glyphs('subtitle', 1));
  list.push(rect('separator', 8, 52, 632, 53.5, WIDGET));
  list.push(glyphs('label', 2));
  list.push(fan('text edit', roundedOutline(97, 59, 137, 78, 2), FIELD), glyphs('path', 3));
  list.push(fan('button', roundedOutline(145, 59, 209, 78, 2), state.focus === -2 ? [90, 90, 90] : WIDGET), glyphs('button text', 4));
  list.push(glyphs('label', 5), fan('text edit', roundedOutline(97, 81, 137, 100, 2), FIELD), glyphs('path', 6));
  list.push(fan('button', roundedOutline(145, 81, 209, 100, 2), WIDGET));
  for (let i = 0; i < 3; i++) {
    const top = 119 + i * 21;
    list.push(fan('checkbox', roundedOutline(16, top, 28, top + 12, 2), WIDGET));
    if (state.checks[i]) list.push(triangle('check mark', [{ x: 18, y: top + 6 }, { x: 21, y: top + 11 }, { x: 27, y: top + 1 }], HANDLE));
    list.push(glyphs('checkbox label', 7 + i));
  }
  list.push(fan('combo box', roundedOutline(16, 178, 115, 197, 2), WIDGET), glyphs('combo text', 10));
  list.push(triangle('combo arrow', [{ x: 98, y: 184 }, { x: 110, y: 184 }, { x: 104, y: 191 }], HANDLE));
  list.push(glyphs('label', 11));
  list.push(fan('radio', circleOutline(22, 209.5, 6), WIDGET), glyphs('radio label', 12));
  list.push(fan('button', roundedOutline(8, 233, 62, 252, 2), WIDGET), glyphs('button text', 13));
  list.push(fan('button', roundedOutline(70, 233, 134, 252, 2), state.saving ? FILL : WIDGET), glyphs('button text', 14));
  list.push(fan('button', roundedOutline(143, 233, 200, 252, 2), WIDGET), glyphs('button text', 15));
  list.push(glyphs('collapsing header', 16), glyphs('controller legend', 17));

  if (state.windowOpen) {
    const windowLabels = labels(state).filter((item) => item.window);
    const wtext = (name, index) => text(name, windowLabels[index].box);
    const outer = roundedOutline(150, 81, 494, 268, 6, undefined, 4);
    // egui's window shadow: a soft band around the window, opaque at the frame and clear 16 px out.
    const shadowInner = roundedOutline(152, 85, 498, 272, 6, undefined, 4);
    const shadowOuter = roundedOutline(138, 73, 512, 288, 20, undefined, 4);
    list.push(ring('window shadow', shadowInner, shadowOuter, premultiply(0, 0, 0, 120), [0, 0, 0, 0], 'shadow gradient → per pixel'));
    list.push(fan('window frame', outer, PANEL));
    list.push(fan('title bar', roundedOutline(150, 81, 494, 114, 6, [true, true, false, false]), TITLE_BAR));
    // Feathering: egui anti-aliases the frame's edge with a one-pixel strip of translucent triangles.
    const featherOuter = roundedOutline(150 - UNIT, 81 - UNIT, 494 + UNIT, 268 + UNIT, 6 + UNIT, undefined, 4);
    list.push(ring('window edge', outer, featherOuter, premultiply(...WIDGET), [0, 0, 0, 0], 'feathered edge → per pixel'));
    list.push(wtext('title', 0), rect('separator', 151, 114, 493, 115.5, WIDGET), wtext('text', 1));
    for (let i = 0; i < 3; i++) {
      const cy = 147.5 + i * 21;
      list.push(fan('radio', circleOutline(164, cy, 6.5), state.focus === i ? [80, 110, 130] : WIDGET));
      if (state.radio === i) list.push(fan('radio dot', circleOutline(164, cy, 3.5, 8), HANDLE));
      list.push(wtext('radio label', 2 + i));
    }
    const sliderX = 157 + (state.slider / 1) * 90;
    list.push(rect('slider rail', 157, 207, 257, 214, WIDGET));
    list.push(fan('slider handle', roundedOutline(sliderX - 5, 202, sliderX + 6, 219, 2), state.focus === 3 ? [220, 230, 240] : HANDLE));
    list.push(fan('drag value', roundedOutline(264, 202, 306, 220, 2), WIDGET), wtext('value', 5), wtext('label', 6));
    const barRight = 157 + 330 * clamp(state.progress, 0, 1);
    list.push(fan('progress rail', roundedOutline(157, 223, 487, 241, 9, undefined, 4), FIELD));
    if (barRight > 170) list.push(fan('progress fill', roundedOutline(157, 223, barRight, 241, 9, undefined, 4), FILL));
    list.push(wtext('percentage', 7));
    list.push(fan('button', roundedOutline(157, 244, 183, 262, 2), state.focus === 4 ? [80, 110, 130] : WIDGET), wtext('button text', 8));
    list.push(fan('button', roundedOutline(191, 244, 237, 262, 2), state.focus === 5 ? [80, 110, 130] : WIDGET), wtext('button text', 9));
    if (state.focus >= 0) {
      const boxes = [[156, 140, 230, 155], [156, 161, 230, 176], [156, 182, 230, 197], [sliderX - 8, 199, sliderX + 9, 222], [154, 241, 186, 265], [188, 241, 240, 265]];
      const [x0, y0, x1, y1] = boxes[state.focus];
      list.push(ring('focus ring', roundedOutline(x0, y0, x1, y1, 3), roundedOutline(x0 - 6, y0 - 6, x1 + 6, y1 + 6, 6), premultiply(120, 200, 255, 230), [0, 0, 0, 0], 'focus stroke → per pixel'));
    }
  }
  return list;
}

// A draw list rasterized into writes, in the order and with the timing of each path. Every write
// carries the cell's value after it, so playback is a sequence of stores.
export function rasterize(state, fontFamily, previous) {
  const atlas = glyphAtlas(state, fontFamily);
  const pixels = previous ? previous.slice() : new Uint8Array(CELLS * 4);
  const calls = [];
  const cells = [];
  const values = [];
  const units = [];
  const kinds = [];
  for (const draw of drawList(state)) {
    const call = { draw, first: cells.length, count: 0, written: 0, units: 0, rows: [] };
    let unit = 0;
    const store = (x, y, kind) => {
      const i = (y * GRID_W + x) * 4;
      cells.push(y * GRID_W + x);
      values.push(((pixels[i] << 24) | (pixels[i + 1] << 16) | (pixels[i + 2] << 8) | pixels[i + 3]) >>> 0);
      units.push(unit);
      kinds.push(kind);
      if (kind === 0) call.written++;
    };
    if (draw.kind === 'clear') {
      for (let y = 0; y < GRID_H; y++) {
        for (let x = 0; x < GRID_W; x++) {
          const i = (y * GRID_W + x) * 4;
          pixels.set(draw.color, i);
          pixels[i + 3] = 255;
          store(x, y, 0);
        }
        unit += 0.25;
      }
    } else if (draw.path === 'spans' && draw.kind === 'rect') {
      const [xa, xb] = rectRange(draw.box[0], draw.box[2], GRID_W);
      const [ya, yb] = rectRange(draw.box[1], draw.box[3], GRID_H);
      for (let y = ya; y <= yb; y++) {
        for (let x = xa; x <= xb; x++) {
          blend(pixels, (y * GRID_W + x) * 4, ...draw.color);
          store(x, y, 0);
        }
        unit += 1;
      }
    } else if (draw.path === 'texels') {
      const [xa, xb] = rectRange(draw.box[0], draw.box[2], GRID_W);
      const [ya, yb] = rectRange(draw.box[1], draw.box[3], GRID_H);
      for (let y = ya; y <= yb; y++) {
        let any = false;
        for (let x = xa; x <= xb; x++) {
          const o = (y * GRID_W + x) * 4;
          if (atlas[o + 3] === 0) continue;
          blend(pixels, o, atlas[o], atlas[o + 1], atlas[o + 2], atlas[o + 3]);
          store(x, y, 0);
          any = true;
        }
        if (any) unit += 1;
      }
    } else if (draw.path === 'spans') {
      // One colour: gather the covered pixels of every triangle, then fill them row by row.
      const rows = new Map();
      for (const [a, b, c] of draw.triangles) {
        triangleCoverage(a, b, c, (x, y, covered) => {
          if (!covered) return;
          if (!rows.has(y)) rows.set(y, []);
          rows.get(y).push(x);
        });
      }
      for (const y of [...rows.keys()].sort((p, q) => p - q)) {
        for (const x of rows.get(y).sort((p, q) => p - q)) {
          blend(pixels, (y * GRID_W + x) * 4, ...draw.color);
          store(x, y, 0);
        }
        unit += 1;
      }
    } else {
      // Mixed vertex colours: the coverage scan finds each row's first covered pixel, then every
      // covered pixel gets its own interpolated colour.
      draw.triangles.forEach(([a, b, c], t) => {
        const [ca, cb, cc] = draw.colors[t];
        let scanning = -1;
        triangleCoverage(a, b, c, (x, y, covered, w0, w1, w2) => {
          if (y !== scanning) scanning = y;
          if (!covered) {
            unit += 0.08;
            store(x, y, 1);
            return;
          }
          const color = [0, 1, 2, 3].map((k) => clamp(Math.round(w0 * ca[k] + w1 * cb[k] + w2 * cc[k]), 0, 255));
          color[0] = Math.min(color[0], 255);
          blend(pixels, (y * GRID_W + x) * 4, color[0], color[1], color[2], color[3]);
          store(x, y, 0);
          unit += 1;
        });
      });
    }
    call.count = cells.length - call.first;
    call.units = Math.max(unit, 1);
    calls.push(call);
  }
  return {
    state,
    calls,
    cells: Int32Array.from(cells),
    values: Uint32Array.from(values),
    units: Float32Array.from(units),
    kinds: Uint8Array.from(kinds),
    pixels,
    hash: fnv1a(pixels),
  };
}

// Timing: how long each call takes on the wall. Slow passes read as a walkthrough; fast ones run
// the frame the way it really runs, as one quick sweep.
function schedule(frame, total, minimum) {
  const weight = (call) => {
    const path = call.draw.path;
    if (path === 'overwrite') return 5;
    if (path === 'pixels') return 1.2 + Math.sqrt(call.written + 1) * 0.55;
    return 0.9 + Math.sqrt(call.count + 1) * 0.28;
  };
  const weights = frame.calls.map(weight);
  const sum = weights.reduce((a, b) => a + b, 0);
  let clock = 0;
  const times = new Float32Array(frame.cells.length);
  for (let c = 0; c < frame.calls.length; c++) {
    const call = frame.calls[c];
    const duration = Math.max(minimum, weights[c] / sum * total);
    call.start = clock;
    call.duration = duration;
    const lead = Math.min(0.18, duration * 0.3);
    for (let k = 0; k < call.count; k++) {
      const index = call.first + k;
      times[index] = clock + lead + (frame.units[index] / call.units) * (duration - lead);
    }
    clock += duration;
  }
  frame.times = times;
  frame.duration = clock;
  return frame;
}

// Shaders ------------------------------------------------------------------------------------------

const SCRUB = /* glsl */ `
  vec3 scrub(vec3 c) {
    if (any(isnan(c)) || any(isinf(c))) return vec3(0.0);
    return clamp(c, 0.0, 64.0);
  }
`;

const FULLSCREEN_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position.xy, 0.0, 1.0);
  }
`;

const SKY_FRAGMENT = /* glsl */ `
  uniform vec3 uTop;
  uniform vec3 uBottom;
  uniform vec3 uAccent;
  uniform vec2 uResolution;
  uniform vec2 uCenter;
  uniform float uRadius;
  uniform float uTime;
  uniform float uDpr;
  uniform vec4 uCalm;
  varying vec2 vUv;
  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }
  void main() {
    vec3 color = mix(uBottom, uTop, vUv.y);
    vec2 css = gl_FragCoord.xy / max(uDpr, 0.01);
    float aspect = uResolution.x / max(uResolution.y, 1.0);
    // A field of pixel centres behind everything, each tested now and then as a scan passes.
    vec2 lattice = css / 24.0;
    vec2 cell = floor(lattice);
    vec2 f = fract(lattice) - 0.5;
    float d = length(f);
    float point = 1.0 - smoothstep(0.035, 0.075, d);
    float scan = fract(uTime * 0.045 + hash(vec2(cell.y, 3.0)) * 0.3);
    float row = fract(cell.x / 90.0 - scan);
    float sampled = exp(-row * 60.0) * step(0.55, hash(cell));
    vec2 toCentre = (vUv - uCenter) * vec2(aspect, 1.0);
    float halo = exp(-dot(toCentre, toCentre) / max(uRadius * uRadius, 1e-4));
    // Calm behind the words: full inside their box, fading out over a soft margin around it.
    vec2 outside = max(uCalm.xy - vUv, vec2(0.0)) + max(vUv - uCalm.zw, vec2(0.0));
    float calm = 1.0 - smoothstep(0.0, 0.12, length(outside * vec2(aspect, 1.0)));
    color += uAccent * halo * 0.07;
    color += uAccent * point * (0.022 + sampled * 0.45) * (1.0 - calm * 0.75);
    color *= 1.0 - calm * 0.25;
    gl_FragColor = vec4(color, 1.0);
  }
`;

const WALL_VERTEX = /* glsl */ `
  attribute vec2 aCell;
  uniform sampler2D tColor;
  uniform sampler2D tPulse;
  uniform vec2 uGrid;
  uniform float uTime;
  uniform float uFlatten;
  uniform vec3 uHover;
  uniform float uBase;
  varying vec3 vColor;
  varying vec3 vNormalW;
  varying vec3 vWorld;
  varying vec2 vFaceUv;
  varying float vTop;
  varying float vFlash;
  varying float vPath;
  varying float vProbe;
  varying float vRise;
  void main() {
    vec2 cellUv = (aCell + 0.5) / uGrid;
    vec4 texel = texture2D(tColor, cellUv);
    vec4 pulse = texture2D(tPulse, cellUv);
    float age = uTime - pulse.r;
    float pop = age >= 0.0 ? 2.4 * exp(-age * 2.8) * (1.0 - exp(-age * 28.0)) : 0.0;
    vec2 away = aCell + 0.5 - uHover.xy;
    float hover = uHover.z * 0.75 * exp(-dot(away, away) / 20.0);
    float height = mix(uBase + pop + hover, 0.05, uFlatten);
    vec3 p = position;
    float rise = p.z + 0.5;
    p.z = rise * height;
    vec3 local = vec3(aCell.x + 0.5 - uGrid.x * 0.5 + p.x, uGrid.y * 0.5 - aCell.y - 0.5 + p.y, p.z);
    vec4 world = modelMatrix * vec4(local, 1.0);
    vWorld = world.xyz;
    vNormalW = mat3(modelMatrix) * normal;
    vTop = step(0.5, normal.z);
    vFaceUv = uv;
    vColor = pow(texel.rgb, vec3(2.2));
    vFlash = age >= 0.0 ? exp(-age * 4.5) : 0.0;
    vPath = pulse.g;
    float probeAge = uTime - pulse.b;
    vProbe = probeAge >= 0.0 ? exp(-probeAge * 9.0) : 0.0;
    vRise = rise;
    gl_Position = projectionMatrix * viewMatrix * world;
  }
`;

const WALL_FRAGMENT = /* glsl */ `
  uniform vec3 uLampPos;
  uniform float uLamp;
  uniform float uReach;
  uniform vec3 uKeyDir;
  uniform vec3 uAccent;
  uniform vec3 uWarm;
  uniform float uGain;
  varying vec3 vColor;
  varying vec3 vNormalW;
  varying vec3 vWorld;
  varying vec2 vFaceUv;
  varying float vTop;
  varying float vFlash;
  varying float vPath;
  varying float vProbe;
  varying float vRise;
  ${SCRUB}
  vec3 safeNormalize(vec3 v, vec3 fallback) {
    float l = length(v);
    return l > 1e-5 ? v / l : fallback;
  }
  void main() {
    vec3 n = safeNormalize(vNormalW, vec3(0.0, 0.0, 1.0));
    vec3 view = safeNormalize(cameraPosition - vWorld, n);
    vec3 toLamp = uLampPos - vWorld;
    float falloff = 1.0 / (1.0 + dot(toLamp, toLamp) / max(uReach * uReach, 1e-3));
    vec3 l = safeNormalize(toLamp, n);
    vec3 halfway = safeNormalize(l + view, n);
    vec3 keyHalf = safeNormalize(uKeyDir + view, n);
    float fresnel = pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 4.0);
    vec3 pathTint = vPath < 0.5 ? vec3(0.75, 0.8, 0.85) : (vPath < 1.5 ? uAccent : (vPath < 2.5 ? uWarm : vec3(0.9, 0.95, 1.0)));
    vec3 color;
    if (vTop > 0.5) {
      vec2 d = abs(vFaceUv - 0.5);
      float edgeDistance = max(d.x, d.y);
      float bevel = 1.0 - smoothstep(0.38, 0.5, edgeDistance);
      float lit = 0.62 + 0.38 * bevel;
      color = vColor * uGain * lit;
      // Every cell is tested at its centre, and the centre shows it.
      float centreDistance = length(vFaceUv - 0.5);
      float centre = 1.0 - smoothstep(0.045, 0.1, centreDistance);
      color += uAccent * centre * (0.035 + vProbe * 2.6);
      float spec = pow(max(dot(n, halfway), 0.0), 420.0) * uLamp * 1.8 * falloff;
      float keySpec = pow(max(dot(n, keyHalf), 0.0), 600.0) * 0.35;
      color += (spec + keySpec) * mix(vec3(1.0), uAccent, 0.25) * (0.35 + 0.65 * bevel);
      color += pathTint * vFlash * 1.25 * (0.6 + 0.4 * bevel);
    } else {
      // Smoked glass sides, lit from inside by the cell's own colour near the top.
      vec3 glass = vec3(0.006, 0.009, 0.012);
      color = mix(glass, vColor * uGain * 0.55, smoothstep(0.15, 1.0, vRise));
      float spec = pow(max(dot(n, halfway), 0.0), 40.0) * uLamp * 0.8 * falloff;
      color += spec * vec3(0.8, 0.9, 1.0) * 0.5 + fresnel * uAccent * 0.05;
      color += pathTint * vFlash * 0.55 * vRise;
    }
    gl_FragColor = vec4(scrub(color), 1.0);
  }
`;

const SHARD_VERTEX = /* glsl */ `
  attribute vec3 aBary;
  attribute vec3 aEdges;
  varying vec3 vBary;
  varying vec3 vEdges;
  void main() {
    vBary = aBary;
    vEdges = aEdges;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const SHARD_FRAGMENT = /* glsl */ `
  uniform vec3 uFill;
  uniform vec3 uEdge;
  uniform float uOpacity;
  uniform float uTime;
  varying vec3 vBary;
  varying vec3 vEdges;
  float line(float b, float on) {
    float width = max(fwidth(b) * 1.6, 1e-4);
    return on * (1.0 - smoothstep(0.0, width, b));
  }
  void main() {
    float outline = max(max(line(vBary.x, vEdges.x), line(vBary.y, vEdges.y)), line(vBary.z, vEdges.z));
    float shimmer = 0.75 + 0.25 * sin(uTime * 3.0 + (vBary.x - vBary.y) * 12.0);
    vec3 color = uFill * 0.22 * shimmer + uEdge * outline * 2.4;
    gl_FragColor = vec4(color * uOpacity, 1.0);
  }
`;

const GLOW_FRAGMENT = /* glsl */ `
  uniform vec3 uColor;
  uniform float uOpacity;
  varying vec2 vUv;
  void main() {
    float across = 1.0 - abs(vUv.y - 0.5) * 2.0;
    float along = smoothstep(0.0, 0.08, vUv.x) * (1.0 - smoothstep(0.92, 1.0, vUv.x));
    float core = pow(max(across, 0.0), 3.0);
    gl_FragColor = vec4(uColor * (core * 2.2 + max(across, 0.0) * 0.25) * along * uOpacity, 1.0);
  }
`;

const PROBE_FRAGMENT = /* glsl */ `
  uniform vec3 uColor;
  uniform float uOpacity;
  varying vec2 vUv;
  void main() {
    vec2 p = vUv - 0.5;
    float r = length(p);
    float spot = exp(-r * r * 90.0);
    float ringDistance = (r - 0.32) * 18.0;
    float ring = exp(-ringDistance * ringDistance);
    float cross = (exp(-abs(p.x) * 70.0) + exp(-abs(p.y) * 70.0)) * (1.0 - smoothstep(0.2, 0.5, r));
    gl_FragColor = vec4(uColor * (spot * 3.0 + ring * 0.8 + cross * 0.5) * uOpacity, 1.0);
  }
`;

const PLAIN_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const PRESENT_FRAGMENT = /* glsl */ `
  uniform sampler2D tImage;
  uniform float uOpacity;
  uniform float uReveal;
  varying vec2 vUv;
  void main() {
    vec3 color = pow(texture2D(tImage, vUv).rgb, vec3(2.2));
    float row = 1.0 - vUv.y;
    float shown = step(row, uReveal);
    float front = exp(-abs(row - uReveal) * 60.0) * step(uReveal, 0.999);
    gl_FragColor = vec4(color * 1.6 + vec3(0.3, 0.6, 0.8) * front * 0.8, shown * uOpacity);
  }
`;

const LABEL_FRAGMENT = /* glsl */ `
  uniform sampler2D tImage;
  uniform float uOpacity;
  uniform float uGain;
  varying vec2 vUv;
  void main() {
    vec4 texel = texture2D(tImage, vUv);
    gl_FragColor = vec4(pow(texel.rgb, vec3(2.2)) * texel.a * uOpacity * uGain, 1.0);
  }
`;

const BRIGHT_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform float uThreshold;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec3 c = scrub(texture2D(tInput, vUv).rgb);
    float luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    gl_FragColor = vec4(c * smoothstep(uThreshold, uThreshold + 0.6, luma), 1.0);
  }
`;

const BLUR_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform vec2 uDirection;
  varying vec2 vUv;
  void main() {
    vec3 sum = texture2D(tInput, vUv).rgb * 0.2270270270;
    sum += texture2D(tInput, vUv + uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv - uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv + uDirection * 3.2307692308).rgb * 0.0702702703;
    sum += texture2D(tInput, vUv - uDirection * 3.2307692308).rgb * 0.0702702703;
    gl_FragColor = vec4(sum, 1.0);
  }
`;

// The composite. On a handheld it quantizes to RGB565, as dream-ini's PortMaster build writes a
// 16-bit framebuffer, with ordered dithering, and shows it in whole blocks of pixels.
const COMPOSITE_FRAGMENT = /* glsl */ `
  uniform sampler2D tScene;
  uniform sampler2D tBloomNear;
  uniform sampler2D tBloomFar;
  uniform float uTime;
  uniform vec2 uResolution;
  uniform float uRetro;
  uniform float uBlock;
  uniform float uPower;
  varying vec2 vUv;
  vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
  }
  float bayer(vec2 p) {
    vec2 q = mod(floor(p), 4.0);
    float i = q.x + q.y * 4.0;
    float m = 0.0;
    if (i < 0.5) m = 0.0; else if (i < 1.5) m = 8.0; else if (i < 2.5) m = 2.0; else if (i < 3.5) m = 10.0;
    else if (i < 4.5) m = 12.0; else if (i < 5.5) m = 4.0; else if (i < 6.5) m = 14.0; else if (i < 7.5) m = 6.0;
    else if (i < 8.5) m = 3.0; else if (i < 9.5) m = 11.0; else if (i < 10.5) m = 1.0; else if (i < 11.5) m = 9.0;
    else if (i < 12.5) m = 15.0; else if (i < 13.5) m = 7.0; else if (i < 14.5) m = 13.0; else m = 5.0;
    return (m + 0.5) / 16.0 - 0.5;
  }
  ${SCRUB}
  void main() {
    float row = 1.0 - vUv.y;
    float retro = uRetro > 0.0 ? step(row, uRetro) : 0.0;
    vec2 uv = vUv;
    vec2 block = floor(gl_FragCoord.xy / uBlock);
    if (retro > 0.5) uv = (block * uBlock + 0.5 * uBlock) / uResolution;
    vec3 color = scrub(texture2D(tScene, uv).rgb);
    color += scrub(texture2D(tBloomNear, uv).rgb) * 0.62 + scrub(texture2D(tBloomFar, uv).rgb) * 0.5;
    color = aces(color * 0.95);
    color = pow(color, vec3(1.0 / 2.2));
    if (retro > 0.5) {
      float d = bayer(block);
      vec3 levels = vec3(31.0, 63.0, 31.0);
      color = floor(clamp(color, 0.0, 1.0) * levels + 0.5 + d) / levels;
      // A handheld's LCD: faint gaps between its pixels.
      vec2 inBlock = fract(gl_FragCoord.xy / uBlock);
      float gap = step(0.12, inBlock.x) * step(0.12, inBlock.y);
      color *= mix(0.82, 1.0, gap);
      // Powering off collapses the picture to a line, then a dot, like an old screen.
      float collapse = clamp(uPower, 0.0, 1.0);
      if (collapse < 1.0) {
        vec2 c = abs(vUv - 0.5);
        float band = mix(0.002, 0.5, smoothstep(0.35, 1.0, collapse));
        float slit = mix(0.004, 0.5, smoothstep(0.0, 0.35, collapse));
        float inside = step(c.y, band) * step(c.x, slit);
        color = mix(vec3(0.0), color + vec3(0.22) * (1.0 - collapse), inside);
      }
    } else {
      color += (fract(sin(dot(gl_FragCoord.xy + fract(uTime), vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
    }
    float front = exp(-abs(row - uRetro) * 90.0) * step(0.001, uRetro) * step(uRetro, 0.999);
    color += vec3(0.5, 0.8, 1.0) * front * 0.35;
    gl_FragColor = vec4(color, 1.0);
  }
`;

function fullscreenMaterial(fragmentShader, uniforms) {
  return new THREE.ShaderMaterial({ vertexShader: FULLSCREEN_VERTEX, fragmentShader, uniforms, depthTest: false, depthWrite: false });
}

function additive(fragmentShader, uniforms, vertexShader = PLAIN_VERTEX) {
  return new THREE.ShaderMaterial({ vertexShader, fragmentShader, uniforms, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, extensions: {} });
}

// Canvas labels: the bezel's read-out, the aspect reticle, the handheld's printing ---------------

function canvasTexture(width, height) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.NoColorSpace;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.anisotropy = 4;
  return { canvas, context: canvas.getContext('2d'), texture };
}

// The environment the handheld's plastic reflects: a dim room with two soft panels and the accent.
function environment(renderer, accent) {
  const room = new THREE.Scene();
  room.background = new THREE.Color(0.012, 0.014, 0.018);
  const panel = (color, intensity, w, h, x, y, z) => {
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.MeshBasicMaterial({ color: color.clone().multiplyScalar(intensity), side: THREE.DoubleSide }));
    mesh.position.set(x, y, z);
    mesh.lookAt(0, 0, 0);
    room.add(mesh);
  };
  panel(new THREE.Color(1, 0.97, 0.92), 3.2, 6, 3, -4, 5, 5);
  panel(new THREE.Color(0.7, 0.85, 1), 1.6, 8, 2, 6, 2, -3);
  panel(accent, 2.2, 10, 1.2, 0, -4, 6);
  panel(new THREE.Color(1, 1, 1), 0.6, 20, 20, 0, 12, 0);
  const generator = new THREE.PMREMGenerator(renderer);
  const target = generator.fromScene(room, 0.04);
  generator.dispose();
  return target;
}

// The handheld ------------------------------------------------------------------------------------

// A generic PortMaster-class handheld, built around the wall as its screen: 80x60 units of screen
// in a body twice as wide. Buttons carry the golden frame's controller legend.
const BODY = { width: 188, height: 112, radius: 18, depth: 12, screenY: 5 };

function roundedShape(width, height, radius, x = 0, y = 0) {
  const shape = new THREE.Shape();
  const w = width / 2;
  const h = height / 2;
  shape.moveTo(x - w + radius, y - h);
  shape.lineTo(x + w - radius, y - h);
  shape.quadraticCurveTo(x + w, y - h, x + w, y - h + radius);
  shape.lineTo(x + w, y + h - radius);
  shape.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
  shape.lineTo(x - w + radius, y + h);
  shape.quadraticCurveTo(x - w, y + h, x - w, y + h - radius);
  shape.lineTo(x - w, y - h + radius);
  shape.quadraticCurveTo(x - w, y - h, x - w + radius, y - h);
  return shape;
}

function printTexture(lines, width, height, color, font) {
  const { canvas, context, texture } = canvasTexture(width, height);
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = color;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  for (const line of lines) {
    context.font = line.font || font;
    context.globalAlpha = line.alpha ?? 1;
    if (line.spacing) context.letterSpacing = line.spacing;
    context.fillText(line.text, line.x ?? width / 2, line.y ?? height / 2);
  }
  texture.needsUpdate = true;
  return texture;
}

function buildHandheld(accent, fontMono) {
  const group = new THREE.Group();
  const buttons = [];
  const plastic = new THREE.MeshStandardMaterial({ color: new THREE.Color(0.018, 0.021, 0.026), roughness: 0.42, metalness: 0, envMapIntensity: 0.6 });
  const trim = new THREE.MeshStandardMaterial({ color: accent.clone().multiplyScalar(0.55), roughness: 0.28, metalness: 0.5, envMapIntensity: 1.2 });
  const rubber = new THREE.MeshStandardMaterial({ color: new THREE.Color(0.006, 0.007, 0.008), roughness: 0.8, envMapIntensity: 0.4 });
  const padMaterial = new THREE.MeshStandardMaterial({ color: new THREE.Color(0.1, 0.105, 0.115), roughness: 0.34, envMapIntensity: 1.0 });
  const glass = new THREE.MeshStandardMaterial({ color: new THREE.Color(0.004, 0.005, 0.006), roughness: 0.06, metalness: 0, envMapIntensity: 1.6 });

  // The shell, with a window for the screen.
  const outline = roundedShape(BODY.width, BODY.height, BODY.radius);
  const screenHole = roundedShape(98, 76, 7, 0, BODY.screenY);
  outline.holes.push(screenHole);
  const shell = new THREE.Mesh(new THREE.ExtrudeGeometry(outline, { depth: BODY.depth, bevelEnabled: true, bevelThickness: 3.5, bevelSize: 3, bevelSegments: 5, curveSegments: 24 }), plastic);
  shell.position.z = -BODY.depth - 3.5;
  group.add(shell);
  // The glass bezel the wall sits under, and a thin accent ring around it.
  const bezel = new THREE.Mesh(new THREE.ShapeGeometry(roundedShape(97, 75, 6.5, 0, BODY.screenY), 16), glass);
  bezel.position.z = -1.2;
  group.add(bezel);
  const ringShape = roundedShape(101, 79, 8, 0, BODY.screenY);
  ringShape.holes.push(roundedShape(98, 76, 7, 0, BODY.screenY));
  const ringMesh = new THREE.Mesh(new THREE.ExtrudeGeometry(ringShape, { depth: 0.8, bevelEnabled: false, curveSegments: 16 }), trim);
  ringMesh.position.z = 0.2;
  group.add(ringMesh);

  const addButton = (mesh, action, travel = 1.4) => {
    mesh.userData = { action, travel, rest: mesh.position.z, press: 0 };
    buttons.push(mesh);
    group.add(mesh);
    return mesh;
  };

  // D-pad: a cross in a shallow well.
  const well = new THREE.Mesh(new THREE.CylinderGeometry(17, 17, 1.2, 48), rubber);
  well.rotation.x = Math.PI / 2;
  well.position.set(-68, -8, 0.2);
  group.add(well);
  const padShape = new THREE.Shape();
  const arm = 4.4;
  const reach = 13;
  padShape.moveTo(-arm, reach);
  padShape.lineTo(arm, reach);
  padShape.lineTo(arm, arm);
  padShape.lineTo(reach, arm);
  padShape.lineTo(reach, -arm);
  padShape.lineTo(arm, -arm);
  padShape.lineTo(arm, -reach);
  padShape.lineTo(-arm, -reach);
  padShape.lineTo(-arm, -arm);
  padShape.lineTo(-reach, -arm);
  padShape.lineTo(-reach, arm);
  padShape.lineTo(-arm, arm);
  padShape.closePath();
  const padGeometry = new THREE.ExtrudeGeometry(padShape, { depth: 3, bevelEnabled: true, bevelThickness: 1, bevelSize: 0.9, bevelSegments: 3 });
  const pad = new THREE.Mesh(padGeometry, padMaterial);
  pad.position.set(-68, -8, 0.9);
  group.add(pad);
  // The pad's four directions are separate hit areas over the one cross.
  for (const [dx, dy, action] of [[0, 1, 'up'], [0, -1, 'down'], [-1, 0, 'left'], [1, 0, 'right']]) {
    const hit = new THREE.Mesh(new THREE.BoxGeometry(9, 9, 6), new THREE.MeshBasicMaterial({ visible: false }));
    hit.position.set(-68 + dx * 8.5, -8 + dy * 8.5, 3);
    hit.userData = { action, travel: 0, rest: 3, press: 0, pad: true };
    buttons.push(hit);
    group.add(hit);
  }
  group.userData.pad = pad;

  // Face buttons: X on top, A to the right, B below, Y to the left, printed like the legend.
  const faceColors = { X: new THREE.Color(0.2, 0.34, 0.62), A: new THREE.Color(0.66, 0.2, 0.22), B: new THREE.Color(0.72, 0.58, 0.16), Y: new THREE.Color(0.18, 0.5, 0.3) };
  for (const [letter, dx, dy, action] of [['X', 0, 1, 'x'], ['A', 1, 0, 'a'], ['B', 0, -1, 'b'], ['Y', -1, 0, 'y']]) {
    const cap = new THREE.Mesh(new THREE.CylinderGeometry(5.2, 5.4, 3.4, 40), new THREE.MeshStandardMaterial({ color: faceColors[letter].clone().multiplyScalar(0.8), roughness: 0.24, envMapIntensity: 1.1 }));
    cap.rotation.x = Math.PI / 2;
    cap.position.set(68 + dx * 11, -8 + dy * 11, 1.9);
    addButton(cap, action);
    const print = new THREE.Mesh(new THREE.PlaneGeometry(7, 7), new THREE.MeshBasicMaterial({ map: printTexture([{ text: letter }], 64, 64, 'rgba(235,240,245,0.92)', `700 40px ${fontMono}`), transparent: true, depthWrite: false }));
    print.position.set(0, 1.75, 0);
    print.rotation.x = -Math.PI / 2;
    cap.add(print);
  }

  // Select and Start.
  for (const [dx, label, action] of [[-10, 'SELECT', 'select'], [10, 'START', 'start']]) {
    const pill = new THREE.Mesh(new THREE.CapsuleGeometry(2.1, 7, 6, 18), padMaterial);
    pill.rotation.z = Math.PI / 2 - 0.35;
    pill.scale.set(1, 1, 0.55);
    pill.position.set(dx, -44, 0.9);
    addButton(pill, action, 0.8);
    const print = new THREE.Mesh(new THREE.PlaneGeometry(16, 3.6), new THREE.MeshBasicMaterial({ map: printTexture([{ text: label }], 256, 56, 'rgba(160,170,180,0.8)', `600 30px ${fontMono}`), transparent: true, depthWrite: false }));
    print.position.set(dx, -50.5, 0.25);
    group.add(print);
  }

  // Speaker grille and shoulder buttons.
  const holeGeometry = new THREE.CylinderGeometry(0.9, 0.9, 0.6, 12);
  for (let r = 0; r < 4; r++) {
    for (let c = 0; c < 5; c++) {
      const hole = new THREE.Mesh(holeGeometry, rubber);
      hole.rotation.x = Math.PI / 2;
      hole.position.set(56 + c * 4.2 + (r % 2) * 2.1, -36 - r * 3.6, 0.1);
      group.add(hole);
    }
  }
  for (const side of [-1, 1]) {
    const shoulder = new THREE.Mesh(new THREE.ExtrudeGeometry(roundedShape(40, 8, 3.5), { depth: 8, bevelEnabled: true, bevelThickness: 1, bevelSize: 1, bevelSegments: 3 }), plastic);
    shoulder.position.set(side * 62, BODY.height / 2 - 0.5, -13);
    group.add(shoulder);
  }

  // The printing: the maker's line over the screen, the screen's format beneath it.
  const brand = new THREE.Mesh(new THREE.PlaneGeometry(92, 6), new THREE.MeshBasicMaterial({
    map: printTexture([{ text: 'dream · soft · render', font: `600 34px ${fontMono}`, spacing: '6px' }], 1024, 64, 'rgba(200,210,220,0.75)'),
    transparent: true, depthWrite: false,
  }));
  brand.position.set(0, BODY.screenY + 44.5, 0.3);
  group.add(brand);
  const format = new THREE.Mesh(new THREE.PlaneGeometry(92, 4.2), new THREE.MeshBasicMaterial({
    map: printTexture([{ text: 'RGB565 · 80×60 · NO GPU REQUIRED', font: `500 26px ${fontMono}`, spacing: '4px' }], 1024, 48, 'rgba(150,165,180,0.7)'),
    transparent: true, depthWrite: false,
  }));
  format.position.set(0, BODY.screenY - 42.5, 0.3);
  group.add(format);

  // The frame LED: bright when a frame is drawn, dim while frames are skipped.
  const led = new THREE.Mesh(new THREE.SphereGeometry(1.3, 16, 12), new THREE.MeshBasicMaterial({ color: new THREE.Color(0.1, 0.5, 0.25) }));
  led.position.set(-60, BODY.screenY + 28, 0.6);
  group.add(led);
  group.userData.led = led;
  group.userData.materials = [plastic, trim, rubber, glass, padMaterial];
  return { group, buttons };
}

// Layout -------------------------------------------------------------------------------------------

function textRect(hero) {
  const text = hero.querySelector('.dw-hero__text') || hero.querySelector('.dw-shell');
  if (!text) return null;
  return text.getBoundingClientRect();
}

function slot(root) {
  const hero = root.closest('.dw-hero') || root.parentElement;
  const box = root.getBoundingClientRect();
  const figure = hero.querySelector('.dw-hero__figure');
  if (figure) {
    const rect = figure.getBoundingClientRect();
    if (rect.width > 40 && rect.height > 40) return { x: rect.left - box.left, y: rect.top - box.top, width: rect.width, height: rect.height, box };
  }
  const width = Math.min(box.width * 0.42, 520);
  return { x: box.width - width - 40, y: (box.height - width * 0.75) / 2, width, height: width * 0.75, box };
}

// The viewport's shape, against the handheld's 4:3.
function aspectCloseness() {
  const ratio = innerWidth / Math.max(1, innerHeight);
  return Math.abs(Math.log(ratio / TARGET_ASPECT));
}

// The scene ------------------------------------------------------------------------------------------

function mount(root) {
  const hero = root.closest('.dw-hero') || root;
  const figureImage = hero.querySelector('.dw-hero__figure img');
  const canvas = document.createElement('canvas');
  canvas.className = 'dsr-hero__canvas';
  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance' });
  } catch {
    return;
  }
  if (!renderer.capabilities.isWebGL2) {
    renderer.dispose();
    return;
  }
  renderer.autoClear = false;
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
  root.append(canvas);
  root.dataset.dsrMode = 'wall';

  const floatTargets = renderer.extensions.has('EXT_color_buffer_float') || renderer.extensions.has('EXT_color_buffer_half_float');
  const targetType = floatTargets ? THREE.HalfFloatType : THREE.UnsignedByteType;
  const makeTarget = () => new THREE.WebGLRenderTarget(1, 1, { type: targetType, depthBuffer: false });
  const sceneTarget = new THREE.WebGLRenderTarget(1, 1, { type: targetType, samples: 4 });
  const bloomTargets = [makeTarget(), makeTarget(), makeTarget(), makeTarget()];

  const accent = cssColor('--dw-accent', '#7fd0e6');
  const warm = new THREE.Color(1.0, 0.72, 0.42);
  const topColor = cssColor('--dw-bg-1', '#101820');
  const bottomColor = cssColor('--dw-bg-0', '#0a1016');
  const fontMono = cssValue('--dw-font-mono', 'ui-monospace, monospace');
  const fontSans = cssValue('--dw-font-sans', 'system-ui, sans-serif');
  const accentCss = cssValue('--dw-accent', '#7fd0e6');
  const mutedCss = cssValue('--dw-text-muted', '#9aa7b0');

  const camera = new THREE.PerspectiveCamera(28, 1, 1, 4000);
  camera.position.set(0, 0, 400);
  camera.lookAt(0, 0, 0);

  const scene = new THREE.Scene();
  const envTarget = environment(renderer, accent);
  scene.environment = envTarget.texture;

  const quad = new THREE.PlaneGeometry(2, 2);
  const skyUniforms = {
    uTop: { value: topColor },
    uBottom: { value: bottomColor },
    uAccent: { value: accent },
    uResolution: { value: new THREE.Vector2(1, 1) },
    uCenter: { value: new THREE.Vector2(0.75, 0.5) },
    uRadius: { value: 0.3 },
    uTime: { value: 0 },
    uDpr: { value: 1 },
    uCalm: { value: new THREE.Vector4(0, 0, 0, 0) },
  };
  const sky = new THREE.Mesh(quad, fullscreenMaterial(SKY_FRAGMENT, skyUniforms));
  sky.frustumCulled = false;
  sky.renderOrder = -10;
  scene.add(sky);

  // The wall: one instanced box per cell. Its colours and write times are textures the shader reads.
  const surface = new Uint8Array(CELLS * 4);
  for (let i = 0; i < CELLS; i++) surface[i * 4 + 3] = 255;
  const colorTexture = new THREE.DataTexture(surface, GRID_W, GRID_H, THREE.RGBAFormat, THREE.UnsignedByteType);
  colorTexture.magFilter = THREE.NearestFilter;
  colorTexture.minFilter = THREE.NearestFilter;
  colorTexture.flipY = false;
  colorTexture.needsUpdate = true;
  const pulses = new Float32Array(CELLS * 4).fill(-100);
  const pulseTexture = new THREE.DataTexture(pulses, GRID_W, GRID_H, THREE.RGBAFormat, THREE.FloatType);
  pulseTexture.magFilter = THREE.NearestFilter;
  pulseTexture.minFilter = THREE.NearestFilter;
  pulseTexture.needsUpdate = true;

  const box = new THREE.BoxGeometry(0.84, 0.84, 1);
  const wallGeometry = new THREE.InstancedBufferGeometry();
  wallGeometry.index = box.index;
  wallGeometry.setAttribute('position', box.getAttribute('position'));
  wallGeometry.setAttribute('normal', box.getAttribute('normal'));
  wallGeometry.setAttribute('uv', box.getAttribute('uv'));
  const cellCoordinates = new Float32Array(CELLS * 2);
  for (let y = 0; y < GRID_H; y++) {
    for (let x = 0; x < GRID_W; x++) {
      cellCoordinates[(y * GRID_W + x) * 2] = x;
      cellCoordinates[(y * GRID_W + x) * 2 + 1] = y;
    }
  }
  wallGeometry.setAttribute('aCell', new THREE.InstancedBufferAttribute(cellCoordinates, 2));
  wallGeometry.instanceCount = CELLS;
  const wallUniforms = {
    tColor: { value: colorTexture },
    tPulse: { value: pulseTexture },
    uGrid: { value: new THREE.Vector2(GRID_W, GRID_H) },
    uTime: { value: 0 },
    uFlatten: { value: 0 },
    uHover: { value: new THREE.Vector3(-99, -99, 0) },
    uBase: { value: 0.34 },
    uLampPos: { value: new THREE.Vector3(0, 0, 60) },
    uLamp: { value: 0 },
    uReach: { value: 30 },
    uKeyDir: { value: new THREE.Vector3(-0.45, 0.6, 0.66).normalize() },
    uAccent: { value: accent },
    uWarm: { value: warm },
    uGain: { value: 2.3 },
  };
  const wall = new THREE.Mesh(wallGeometry, new THREE.ShaderMaterial({ vertexShader: WALL_VERTEX, fragmentShader: WALL_FRAGMENT, uniforms: wallUniforms }));
  wall.frustumCulled = false;

  // The wall's frame: a dark rim a little proud of the cells.
  const frameShape = roundedShape(GRID_W + 5, GRID_H + 5, 2.4);
  frameShape.holes.push(roundedShape(GRID_W + 1, GRID_H + 1, 0.8));
  const frameMaterial = new THREE.MeshStandardMaterial({ color: new THREE.Color(0.01, 0.012, 0.015), roughness: 0.46, metalness: 0.25, envMapIntensity: 0.2 });
  const frame = new THREE.Mesh(new THREE.ExtrudeGeometry(frameShape, { depth: 1.4, bevelEnabled: true, bevelThickness: 0.4, bevelSize: 0.4, bevelSegments: 3, curveSegments: 10 }), frameMaterial);
  frame.position.z = -0.9;
  const backplate = new THREE.Mesh(new THREE.PlaneGeometry(GRID_W + 1, GRID_H + 1), new THREE.MeshStandardMaterial({ color: new THREE.Color(0.004, 0.005, 0.007), roughness: 0.9 }));
  backplate.position.z = -0.02;

  // The present: the crate's own output, the golden PNG, laid over the flattened wall.
  const presentUniforms = { tImage: { value: null }, uOpacity: { value: 0 }, uReveal: { value: 0 } };
  const present = new THREE.Mesh(new THREE.PlaneGeometry(GRID_W, GRID_H), new THREE.ShaderMaterial({ vertexShader: PLAIN_VERTEX, fragmentShader: PRESENT_FRAGMENT, uniforms: presentUniforms, transparent: true, depthWrite: false }));
  present.position.z = 0.25;
  present.renderOrder = 4;
  present.visible = false;
  if (figureImage) {
    new THREE.TextureLoader().load(figureImage.currentSrc || figureImage.src, (texture) => {
      texture.colorSpace = THREE.NoColorSpace;
      texture.minFilter = THREE.LinearMipmapLinearFilter;
      texture.anisotropy = 8;
      presentUniforms.tImage.value = texture;
    });
  }

  // The shard: the call being rasterized, as glass above the wall; its edges are drawn from
  // barycentric coordinates, so a fan shows its spokes and a rectangle only its outline.
  const shardUniforms = { uFill: { value: new THREE.Color() }, uEdge: { value: accent.clone() }, uOpacity: { value: 0 }, uTime: { value: 0 } };
  const shardMaterial = additive(SHARD_FRAGMENT, shardUniforms, SHARD_VERTEX);
  shardMaterial.side = THREE.DoubleSide;
  const shard = new THREE.Mesh(new THREE.BufferGeometry(), shardMaterial);
  shard.renderOrder = 6;
  shard.frustumCulled = false;

  // The row being filled, and the coverage probe with its thread down to the pixel under test.
  const cursorUniforms = { uColor: { value: accent.clone() }, uOpacity: { value: 0 } };
  const cursor = new THREE.Mesh(new THREE.PlaneGeometry(GRID_W + 3, 1.6), additive(GLOW_FRAGMENT, cursorUniforms));
  cursor.renderOrder = 5;
  const probeUniforms = { uColor: { value: warm.clone() }, uOpacity: { value: 0 } };
  const probe = new THREE.Mesh(new THREE.PlaneGeometry(3.2, 3.2), additive(PROBE_FRAGMENT, probeUniforms));
  probe.renderOrder = 5;

  // The bezel read-out and the aspect reticle.
  const hud = canvasTexture(2560, 96);
  const hudUniforms = { tImage: { value: hud.texture }, uOpacity: { value: 1 }, uGain: { value: 1.6 } };
  const hudMesh = new THREE.Mesh(new THREE.PlaneGeometry(GRID_W + 5, (GRID_W + 5) * 96 / 2560), additive(LABEL_FRAGMENT, hudUniforms));
  hudMesh.position.set(0, -GRID_H / 2 - 4.6, 0.3);
  const reticle = canvasTexture(128, 128);
  const reticleUniforms = { tImage: { value: reticle.texture }, uOpacity: { value: 1 }, uGain: { value: 1.5 } };
  const reticleMesh = new THREE.Mesh(new THREE.PlaneGeometry(4.4, 4.4), additive(LABEL_FRAGMENT, reticleUniforms));
  reticleMesh.position.set(GRID_W / 2 - 1.2, GRID_H / 2 + 3.9, 0.3);

  const wallGroup = new THREE.Group();
  wallGroup.add(backplate, wall, frame, present, shard, cursor, probe, hudMesh, reticleMesh);
  scene.add(wallGroup);

  // The handheld, hidden until the viewport takes its shape.
  const handheld = buildHandheld(accent, fontMono);
  handheld.group.visible = false;
  scene.add(handheld.group);

  const key = new THREE.DirectionalLight(new THREE.Color(1.0, 0.95, 0.9), 1.6);
  key.position.set(-120, 200, 300);
  const rimLight = new THREE.DirectionalLight(accent, 1.4);
  rimLight.position.set(220, 60, -120);
  const lamp = new THREE.PointLight(new THREE.Color(0.85, 0.95, 1.0), 0, 0, 2);
  scene.add(key, rimLight, lamp);

  // Post-processing.
  const postScene = new THREE.Scene();
  const postCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const postQuad = new THREE.Mesh(quad);
  postQuad.frustumCulled = false;
  postScene.add(postQuad);
  const brightMaterial = fullscreenMaterial(BRIGHT_FRAGMENT, { tInput: { value: sceneTarget.texture }, uThreshold: { value: 0.85 } });
  const blurMaterial = fullscreenMaterial(BLUR_FRAGMENT, { tInput: { value: null }, uDirection: { value: new THREE.Vector2() } });
  const copyMaterial = fullscreenMaterial(/* glsl */ `
    uniform sampler2D tInput;
    varying vec2 vUv;
    void main() { gl_FragColor = texture2D(tInput, vUv); }
  `, { tInput: { value: null } });
  const compositeUniforms = {
    tScene: { value: sceneTarget.texture },
    tBloomNear: { value: bloomTargets[0].texture },
    tBloomFar: { value: bloomTargets[2].texture },
    uTime: { value: 0 },
    uResolution: { value: new THREE.Vector2(1, 1) },
    uRetro: { value: 0 },
    uBlock: { value: 4 },
    uPower: { value: 1 },
  };
  const compositeMaterial = fullscreenMaterial(COMPOSITE_FRAGMENT, compositeUniforms);
  function pass(material, target) {
    postQuad.material = material;
    renderer.setRenderTarget(target);
    renderer.render(postScene, postCamera);
  }
  function blur(target, scratch, radius) {
    blurMaterial.uniforms.tInput.value = target.texture;
    blurMaterial.uniforms.uDirection.value.set(radius / target.width, 0);
    pass(blurMaterial, scratch);
    blurMaterial.uniforms.tInput.value = scratch.texture;
    blurMaterial.uniforms.uDirection.value.set(0, radius / target.height);
    pass(blurMaterial, target);
  }

  // Layout: where the wall stands, and the handheld's pose around it.
  let handheldTarget = 0;
  let width = 1;
  let height = 1;
  let dpr = 1;
  const quality = { level: 1, slow: 0 };
  const wallPose = { position: new THREE.Vector3(), quaternion: new THREE.Quaternion(), scale: 1 };
  const handheldPose = { position: new THREE.Vector3(), quaternion: new THREE.Quaternion(), scale: 1 };
  const raycaster = new THREE.Raycaster();
  const tmp = new THREE.Vector3();
  const tmp2 = new THREE.Vector3();
  const ndc = new THREE.Vector2();
  const zPlane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);

  function worldAt(px, py, target) {
    ndc.set(px / width * 2 - 1, -(py / height * 2 - 1));
    raycaster.setFromCamera(ndc, camera);
    raycaster.ray.intersectPlane(zPlane, target);
    return target;
  }

  function layout() {
    const rect = root.getBoundingClientRect();
    width = Math.max(1, Math.round(rect.width));
    height = Math.max(1, Math.round(rect.height));
    const small = Math.min(innerWidth, innerHeight) < 700;
    // The handheld is shown in blocks of pixels anyway, so it renders at half the resolution.
    dpr = Math.min(window.devicePixelRatio || 1, small ? 1.5 : 1.75) * quality.level * (handheldTarget === 1 ? 0.5 : 1);
    renderer.setPixelRatio(dpr);
    renderer.setSize(width, height, false);
    const w = Math.max(1, Math.floor(width * dpr));
    const h = Math.max(1, Math.floor(height * dpr));
    sceneTarget.setSize(w, h);
    bloomTargets[0].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[1].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[2].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    bloomTargets[3].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    compositeUniforms.uResolution.value.set(w, h);
    // Blocks of about 4 CSS pixels on the handheld, which renders at half resolution.
    compositeUniforms.uBlock.value = Math.max(2, Math.round(4 * dpr));
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();
    skyUniforms.uResolution.value.set(width, height);
    skyUniforms.uDpr.value = dpr;

    const spot = slot(root);
    const centre = worldAt(spot.x + spot.width / 2, spot.y + spot.height / 2, new THREE.Vector3());
    const left = worldAt(spot.x, spot.y + spot.height / 2, tmp.clone());
    const right = worldAt(spot.x + spot.width, spot.y + spot.height / 2, tmp2.clone());
    const top = worldAt(spot.x + spot.width / 2, spot.y, new THREE.Vector3());
    const bottom = worldAt(spot.x + spot.width / 2, spot.y + spot.height, new THREE.Vector3());
    const worldWidth = left.distanceTo(right);
    const worldHeight = top.distanceTo(bottom);
    const narrow = width < 700;
    // The wall: turned towards the text a little, tilted back so its cells show their sides.
    wallPose.position.copy(centre);
    wallPose.quaternion.setFromEuler(new THREE.Euler(narrow ? -0.32 : -0.26, narrow ? 0 : -0.3, narrow ? 0 : -0.02));
    wallPose.scale = Math.min(worldWidth / (GRID_W + 5) * (narrow ? 0.93 : 1.0), worldHeight / (GRID_H + 9) * 1.08);
    // The handheld fills the same slot, facing front, screen and all.
    handheldPose.position.copy(centre);
    handheldPose.quaternion.setFromEuler(new THREE.Euler(-0.1, narrow ? 0 : -0.14, 0));
    handheldPose.scale = Math.min(worldWidth / (BODY.width + 8), worldHeight / (BODY.height + 8));

    fitPose(wallPose, spot, (GRID_W + 5) / 2, (GRID_H + 5) / 2 + 3, 6, narrow ? 0.98 : 0.97);
    fitPose(handheldPose, spot, BODY.width / 2 + 3, BODY.height / 2 + 3, BODY.depth + 3, 0.96);
    skyUniforms.uCenter.value.set((spot.x + spot.width / 2) / width, 1 - (spot.y + spot.height / 2) / height);
    skyUniforms.uRadius.value = Math.max(spot.width, spot.height) / height * 0.7;
    const words = textRect(hero);
    if (words) {
      const box = root.getBoundingClientRect();
      skyUniforms.uCalm.value.set((words.left - box.left) / width, 1 - (words.bottom - box.top) / height, (words.right - box.left) / width, 1 - (words.top - box.top) / height);
    }
    drawReticle();
  }

  // Scale and move a pose until its box, as the camera sees it, fills the slot: a turned wall's
  // near side looks bigger than its far side, so the projected corners decide.
  const corner = new THREE.Vector3();
  const poseMatrix = new THREE.Matrix4();
  function fitPose(target, spot, halfWidth, halfHeight, depth, fill) {
    for (let pass = 0; pass < 3; pass++) {
      poseMatrix.compose(target.position, target.quaternion, new THREE.Vector3(target.scale, target.scale, target.scale));
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (const sx of [-1, 1]) {
        for (const sy of [-1, 1]) {
          for (const sz of [0, 1]) {
            corner.set(sx * halfWidth, sy * halfHeight, sz * depth).applyMatrix4(poseMatrix).project(camera);
            const px = (corner.x + 1) / 2 * width;
            const py = (1 - corner.y) / 2 * height;
            minX = Math.min(minX, px);
            maxX = Math.max(maxX, px);
            minY = Math.min(minY, py);
            maxY = Math.max(maxY, py);
          }
        }
      }
      const factor = Math.min(spot.width * fill / Math.max(1, maxX - minX), spot.height * fill / Math.max(1, maxY - minY));
      target.scale *= factor;
      const centreNow = worldAt((minX + maxX) / 2, (minY + maxY) / 2, new THREE.Vector3());
      const centreWanted = worldAt(spot.x + spot.width / 2, spot.y + spot.height / 2, new THREE.Vector3());
      target.position.add(centreWanted.sub(centreNow));
    }
  }

  // The reticle: the handheld's 4:3, and inside it a shape that follows the viewport's.
  let closeness = 1;
  function drawReticle() {
    closeness = aspectCloseness();
    const { context, canvas: c, texture } = reticle;
    context.clearRect(0, 0, c.width, c.height);
    const near = clamp(1 - closeness / 0.35, 0, 1);
    context.strokeStyle = accentCss;
    context.globalAlpha = 0.28 + near * 0.5;
    context.lineWidth = 5;
    const outer = { w: 104, h: 78 };
    context.strokeRect((128 - outer.w) / 2, (128 - outer.h) / 2, outer.w, outer.h);
    const ratio = innerWidth / Math.max(1, innerHeight);
    const fitW = ratio >= TARGET_ASPECT ? 104 : 78 * ratio;
    const fitH = ratio >= TARGET_ASPECT ? 104 / ratio : 78;
    context.globalAlpha = 0.18 + near * 0.55;
    context.fillStyle = accentCss;
    context.fillRect((128 - fitW) / 2 + 9, (128 - fitH) / 2 + 9, Math.max(2, fitW - 18), Math.max(2, fitH - 18));
    context.globalAlpha = 1;
    texture.needsUpdate = true;
  }

  // The bezel read-out.
  let hudText = '';
  let hudAt = -1;
  let hudCall = -1;
  function drawHud(left, right, urgent = true) {
    const next = `${left}\u0000${right}`;
    if (next === hudText) return;
    if (!urgent && time - hudAt < 0.12) return;
    hudText = next;
    hudAt = time;
    const { context, canvas: c, texture } = hud;
    context.clearRect(0, 0, c.width, c.height);
    context.font = `500 40px ${fontMono}`;
    context.textBaseline = 'middle';
    context.fillStyle = mutedCss;
    context.textAlign = 'left';
    context.fillText(left, 24, 50);
    context.textAlign = 'right';
    context.fillStyle = accentCss;
    context.fillText(right, c.width - 24, 50);
    texture.needsUpdate = true;
  }

  // Playback ------------------------------------------------------------------------------------

  let time = 0;
  let current = null; // the frame being played
  let cursorIndex = 0; // the next write to apply
  let frameStart = 0;
  let lastHash = '';
  let callShown = -1;
  let step = null;
  let stepStart = 0;
  let state = { ...GOLDEN_STATE, checks: [...GOLDEN_STATE.checks] };
  let goldenHash = '';
  const sans = `${fontSans}`;

  // The header's PNG, hashed as the bytes it is.
  if (figureImage && hero.querySelector('[data-hero-project="home"]')) {
    const original = new Image();
    original.decoding = 'async';
    original.onload = () => {
      try {
        const c = document.createElement('canvas');
        c.width = original.naturalWidth;
        c.height = original.naturalHeight;
        const context = c.getContext('2d', { willReadFrequently: true });
        context.drawImage(original, 0, 0);
        goldenHash = fnv1a(context.getImageData(0, 0, c.width, c.height).data);
      } catch {
        goldenHash = '';
      }
    };
    original.src = new URL('media/window.png', location.href).href;
  }

  function stateWith(changes) {
    return { ...state, checks: [...state.checks], ...changes };
  }

  function startDraw(nextState, seconds, minimum) {
    const frameData = rasterize(nextState, sans, surface);
    schedule(frameData, seconds, minimum);
    current = frameData;
    cursorIndex = 0;
    callLookup = 0;
    frameStart = time;
    callShown = -1;
    state = nextState;
    return frameData;
  }

  function applyWrites(upTo) {
    if (!current) return;
    const { cells, values, kinds, times } = current;
    let changed = false;
    while (cursorIndex < cells.length && times[cursorIndex] <= upTo) {
      const cell = cells[cursorIndex];
      const value = values[cursorIndex];
      const o = cell * 4;
      const at = frameStart + times[cursorIndex];
      const call = callOf(cursorIndex);
      if (kinds[cursorIndex] === 0) {
        surface[o] = value >>> 24;
        surface[o + 1] = (value >>> 16) & 255;
        surface[o + 2] = (value >>> 8) & 255;
        surface[o + 3] = 255;
        pulses[o] = at;
        pulses[o + 1] = pathCode(call.draw.path);
        if (call.draw.path === 'pixels') pulses[o + 2] = at;
      } else {
        pulses[o + 2] = at - 0.06;
      }
      changed = true;
      cursorIndex++;
    }
    if (changed) {
      colorTexture.needsUpdate = true;
      pulseTexture.needsUpdate = true;
    }
  }

  let callLookup = 0;
  function callOf(index) {
    const calls = current.calls;
    if (callLookup >= calls.length || index < calls[callLookup].first) callLookup = 0;
    while (callLookup < calls.length - 1 && index >= calls[callLookup].first + calls[callLookup].count) callLookup++;
    return calls[callLookup];
  }

  function pathCode(path) {
    return path === 'overwrite' ? 0 : path === 'spans' ? 1 : path === 'pixels' ? 2 : 3;
  }

  function shardGeometry(draw) {
    const positions = [];
    const bary = [];
    const edges = [];
    for (let t = 0; t < draw.triangles.length; t++) {
      const [a, b, c] = draw.triangles[t];
      for (const p of [a, b, c]) positions.push(p.x - GRID_W / 2, GRID_H / 2 - p.y, 0);
      bary.push(1, 0, 0, 0, 1, 0, 0, 0, 1);
      // A rectangle shows its outline, a fan its rim and spokes, anything else every edge.
      let flags = [1, 1, 1];
      if (draw.kind === 'rect' || draw.kind === 'text' || draw.kind === 'clear') flags = t === 0 ? [1, 0, 1] : [1, 1, 0];
      if (draw.kind === 'fan') flags = [1, 0.35, 0.35];
      for (let k = 0; k < 3; k++) edges.push(...flags);
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute('aBary', new THREE.Float32BufferAttribute(bary, 3));
    geometry.setAttribute('aEdges', new THREE.Float32BufferAttribute(edges, 3));
    return geometry;
  }

  // The show: a slow walk through the frame, the golden PNG presented, an unchanged frame skipped,
  // then the frame as it really runs, changed a little and back, each followed by its skip.
  const plan = [
    { kind: 'draw', state: () => stateWith({ ...GOLDEN_STATE, checks: [...GOLDEN_STATE.checks] }), seconds: 17, minimum: 0.16, golden: true },
    { kind: 'present', seconds: 3.6 },
    { kind: 'skip', seconds: 2.6 },
    { kind: 'draw', state: () => stateWith({ progress: 0.58, radio: 0 }), seconds: 1.8, minimum: 0.012 },
    { kind: 'skip', seconds: 2.2 },
    { kind: 'draw', state: () => stateWith({ progress: 0.77, radio: 1, slider: 0.35, checks: [true, true, false] }), seconds: 1.8, minimum: 0.012 },
    { kind: 'skip', seconds: 2.2 },
    { kind: 'draw', state: () => stateWith({ ...GOLDEN_STATE, checks: [...GOLDEN_STATE.checks] }), seconds: 1.8, minimum: 0.012, golden: true },
    { kind: 'present', seconds: 3.6 },
    { kind: 'skip', seconds: 2.4 },
  ];
  let planIndex = -1;

  function nextStep() {
    planIndex = (planIndex + 1) % plan.length;
    const entry = plan[planIndex];
    step = { ...entry };
    stepStart = time;
    if (entry.kind === 'draw') {
      const frameData = startDraw(entry.state(), entry.seconds, entry.minimum);
      step.seconds = frameData.duration + 0.4;
      step.changed = frameData.hash !== lastHash;
    }
    if (entry.kind === 'skip') {
      step.hash = lastHash;
    }
  }

  // Handheld steps: frames only when a button changes something; otherwise they are skipped.
  function handheldDraw(nextState) {
    const frameData = startDraw(nextState, 0.9, 0.006);
    step = { kind: 'draw', seconds: frameData.duration + 0.15, handheld: true, changed: frameData.hash !== lastHash };
    stepStart = time;
  }

  // The secret: a 4:3 viewport, the shape of the handheld screens the crate was written for.
  let handheldMix = 0;
  let matchTimer = 0;
  let power = 1;
  let powerTarget = 1;
  let rebootAt = -1;
  function checkAspect() {
    drawReticle();
    const matched = closeness < 0.0095;
    clearTimeout(matchTimer);
    matchTimer = setTimeout(() => {
      const wanted = matched ? 1 : 0;
      if (wanted === handheldTarget) return;
      handheldTarget = wanted;
      root.dataset.dsrMode = matched ? 'handheld' : 'wall';
      root.classList.toggle('is-handheld', matched);
      layout();
      if (matched) {
        state = stateWith({ focus: 2 });
        handheldDraw(state);
        power = 1;
        powerTarget = 1;
      } else {
        state = stateWith({ focus: -1 });
        planIndex = plan.length - 1;
        nextStep();
      }
      requestFrame();
    }, 450);
  }

  function press(action) {
    const focusables = 6;
    let next = null;
    if (!state.windowOpen && action !== 'b' && action !== 'select' && action !== 'start') {
      next = stateWith({ windowOpen: true, focus: 2 });
    } else if (action === 'up' || action === 'down') {
      next = stateWith({ focus: (state.focus + (action === 'down' ? 1 : focusables - 1)) % focusables });
    } else if (action === 'left' || action === 'right') {
      if (state.focus === 3) next = stateWith({ slider: clamp(Math.round((state.slider + (action === 'right' ? 0.1 : -0.1)) * 10) / 10, 0, 1) });
      else next = stateWith({ focus: (state.focus + (action === 'right' ? 1 : focusables - 1)) % focusables });
    } else if (action === 'a') {
      if (state.focus >= 0 && state.focus <= 2) next = stateWith({ radio: state.focus });
      else if (state.focus === 4 || state.focus === 5) next = stateWith({ windowOpen: false, focus: -1 });
      else next = stateWith({ slider: state.slider >= 1 ? 0 : clamp(state.slider + 0.2, 0, 1) });
    } else if (action === 'b') {
      next = stateWith({ windowOpen: !state.windowOpen, focus: state.windowOpen ? -1 : 2 });
    } else if (action === 'x') {
      next = stateWith({ checks: [state.checks[0], !state.checks[1], state.checks[2]] });
    } else if (action === 'y') {
      next = stateWith({ saving: !state.saving });
    } else if (action === 'start') {
      next = stateWith({ progress: state.progress >= 0.99 ? 0.02 : clamp(state.progress + 0.12, 0, 1), windowOpen: true });
    } else if (action === 'select') {
      powerTarget = 0;
      rebootAt = time + 2.2;
      return;
    }
    if (next && power > 0.9) handheldDraw(next);
  }

  // The pointer: a lamp over the wall that lifts the cells beneath it; a click redraws the frame
  // at speed, changed, and on the handheld presses whatever button it lands on.
  const pointer = new THREE.Vector2(0, 0);
  let pointerActive = false;
  let lastPointer = 0;
  let presence = 0;
  const lampTarget = new THREE.Vector3();
  const lampPosition = new THREE.Vector3(0, 0, 120);
  const hoverCell = new THREE.Vector2(-99, -99);
  let hoverStrength = 0;
  const inverse = new THREE.Matrix4();
  function onPointer(event) {
    const rect = root.getBoundingClientRect();
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -((event.clientY - rect.top) / rect.height * 2 - 1));
    pointerActive = true;
    lastPointer = performance.now();
    requestFrame();
  }
  function onPress(event) {
    onPointer(event);
    raycaster.setFromCamera(pointer, camera);
    if (handheldMix > 0.95) {
      const hits = raycaster.intersectObjects(handheld.buttons, false);
      if (hits.length) {
        const button = hits[0].object;
        button.userData.press = 1;
        if (button.userData.pad) handheld.group.userData.padPress = { action: button.userData.action, t: 1 };
        press(button.userData.action);
      }
      return;
    }
    wallGroup.updateMatrixWorld();
    inverse.copy(wallGroup.matrixWorld).invert();
    const localRay = raycaster.ray.clone().applyMatrix4(inverse);
    const hit = localRay.intersectPlane(zPlane, tmp);
    if (hit && Math.abs(hit.x) < GRID_W / 2 && Math.abs(hit.y) < GRID_H / 2 && step && step.kind !== 'draw') {
      const changes = { progress: Math.round((0.1 + Math.random() * 0.85) * 100) / 100, radio: Math.floor(Math.random() * 3) };
      planIndex = 2;
      const frameData = startDraw(stateWith(changes), 1.8, 0.012);
      step = { kind: 'draw', seconds: frameData.duration + 0.4, changed: frameData.hash !== lastHash };
      stepStart = time;
    }
  }
  function onLeave() {
    pointerActive = false;
  }
  hero.addEventListener('pointermove', onPointer, { passive: true });
  hero.addEventListener('pointerdown', onPress, { passive: true });
  hero.addEventListener('pointerleave', onLeave, { passive: true });

  // The loop.
  const clock = new THREE.Clock();
  let visible = false;
  let running = false;
  let first = true;
  let lost = false;
  let retroAccumulator = 0;
  const pose = { position: new THREE.Vector3(), quaternion: new THREE.Quaternion(), scale: 1 };
  const screenOffset = new THREE.Vector3();

  if (reduceMotion) {
    // One frame: the golden frame, drawn and at rest.
    const frameData = rasterize({ ...GOLDEN_STATE, checks: [...GOLDEN_STATE.checks] }, sans, surface);
    surface.set(frameData.pixels);
    colorTexture.needsUpdate = true;
    lastHash = frameData.hash;
    drawHud('window · 80×60 · one frame', `fnv-1a ${frameData.hash}`);
  } else {
    drawHud('begin_frame(80, 60)', 'surface · 80×60 · rgba8');
  }

  function frameTick() {
    running = false;
    if (lost) return;
    const rawDt = clock.getDelta();
    // On the handheld, 30 frames a second, as its framebuffer would take them.
    if (handheldMix > 0.5 && !reduceMotion) {
      retroAccumulator += rawDt;
      if (retroAccumulator < 1 / 30) {
        requestFrame();
        return;
      }
    }
    const dt = Math.min(handheldMix > 0.5 ? retroAccumulator : rawDt, 0.1);
    retroAccumulator = 0;
    if (!reduceMotion && rawDt < 0.5 && handheldMix < 0.5) {
      quality.slow = rawDt > 1 / 40 ? quality.slow + rawDt : Math.max(0, quality.slow - rawDt * 0.5);
      if (quality.slow > 1.5 && quality.level > 0.5) {
        quality.level = Math.max(0.5, quality.level - 0.2);
        quality.slow = 0;
        layout();
      }
    }
    if (!reduceMotion) time += dt;
    skyUniforms.uTime.value = time;
    wallUniforms.uTime.value = time;
    shardUniforms.uTime.value = time;
    compositeUniforms.uTime.value = time;

    // Mode blend.
    handheldMix += (handheldTarget - handheldMix) * (reduceMotion ? 1 : Math.min(1, dt * 2.4));
    if (Math.abs(handheldTarget - handheldMix) < 0.002) handheldMix = handheldTarget;
    const blendT = smooth(clamp(handheldMix, 0, 1));
    compositeUniforms.uRetro.value = clamp(handheldMix * 1.02, 0, 1);

    // The show.
    if (!reduceMotion) {
      if (!step) {
        if (time > 0.6) nextStep();
      } else if (time - stepStart >= step.seconds) {
        if (step.kind === 'draw') {
          applyWrites(Infinity);
          lastHash = current.hash;
        }
        if (step.handheld || handheldTarget === 1) {
          step = { kind: 'skip', seconds: 1e9, hash: lastHash, handheld: true };
          stepStart = time;
        } else {
          nextStep();
        }
      }
    }
    let presentOpacity = 0;
    let presentReveal = 0;
    let flatten = 0;
    let shardOpacity = 0;
    let cursorOpacity = 0;
    let probeOpacity = 0;
    if (step && !reduceMotion) {
      const age = time - stepStart;
      if (step.kind === 'draw' && current) {
        const local = time - frameStart;
        applyWrites(local);
        // The call under way: its shard and read-out.
        let active = null;
        for (let c = 0; c < current.calls.length; c++) {
          const call = current.calls[c];
          if (local >= call.start && local < call.start + call.duration) {
            active = c;
            break;
          }
        }
        if (active !== null) {
          const call = current.calls[active];
          if (active !== callShown) {
            callShown = active;
            shard.geometry.dispose();
            shard.geometry = shardGeometry(call.draw);
            const color = call.draw.color || (call.draw.colors ? call.draw.colors[0][0] : [200, 212, 224]);
            shardUniforms.uFill.value.setRGB(color[0] / 255, color[1] / 255, color[2] / 255).convertSRGBToLinear();
            shardUniforms.uEdge.value.copy(call.draw.path === 'pixels' ? warm : accent);
            cursorUniforms.uColor.value.copy(call.draw.path === 'pixels' ? warm : accent);
          }
          const progress = clamp((local - call.start) / call.duration, 0, 1);
          const fast = current.duration < 4;
          shardOpacity = fast ? 0.55 : Math.sin(Math.PI * clamp(progress * 1.15, 0, 1)) * 0.95 + 0.05;
          shard.position.z = mix(8, 2.2, smooth(clamp(progress * 1.6, 0, 1)));
          // The row: the latest write's row, where the fill has reached.
          const last = Math.max(call.first, Math.min(cursorIndex - 1, call.first + call.count - 1));
          const cell = current.cells[last];
          const row = Math.floor(cell / GRID_W);
          const column = cell % GRID_W;
          cursor.position.set(0, GRID_H / 2 - row - 0.5, 1.4);
          cursorOpacity = (call.draw.path === 'pixels' ? 0.35 : 0.9) * (fast ? 0.5 : 1) * (cursorIndex > call.first ? 1 : 0);
          if (call.draw.path === 'pixels' && cursorIndex > call.first) {
            probe.position.set(column + 0.5 - GRID_W / 2, GRID_H / 2 - row - 0.5, 1.9);
            probeOpacity = fast ? 0.4 : 1;
          }
          const px = call.written;
          const hashText = `frame ${current.hash.slice(0, 4)}… · surface_changed: ${step.changed ? 'true' : 'false'}`;
          drawHud(`call ${String(active + 1).padStart(2, '0')}/${current.calls.length} · ${call.draw.name} · ${call.draw.label} · ${px} px`, fast ? 'render_egui · at speed' : hashText, active !== hudCall);
          hudCall = active;
        }
      } else if (step.kind === 'present') {
        const t = age / step.seconds;
        flatten = smooth(clamp(t * 3.2, 0, 1)) * (1 - smooth(clamp((t - 0.82) * 5.5, 0, 1)));
        presentReveal = clamp((t - 0.12) * 2.2, 0, 1);
        presentOpacity = presentUniforms.tImage.value ? smooth(clamp((t - 0.1) * 4, 0, 1)) * (1 - smooth(clamp((t - 0.8) * 5, 0, 1))) : 0;
        drawHud('present · the golden frame the crate drew · 640×480', goldenHash ? `fnv-1a ${goldenHash} · 1,228,800 bytes` : 'window.png · 640×480');
      } else if (step.kind === 'skip') {
        const sweep = clamp(age / 1.2, 0, 1);
        cursor.position.set(0, GRID_H / 2 - sweep * GRID_H, 1.2);
        cursorUniforms.uColor.value.setRGB(0.35, 0.4, 0.45);
        cursorOpacity = age < 1.2 ? 0.45 : 0;
        drawHud(step.handheld ? 'frame skipped · no input' : 'render_egui · same bytes as the last frame', `fnv-1a ${lastHash} · surface_changed: false`);
      }
    }
    shardUniforms.uOpacity.value += (shardOpacity - shardUniforms.uOpacity.value) * Math.min(1, dt * 14);
    cursorUniforms.uOpacity.value += (cursorOpacity - cursorUniforms.uOpacity.value) * Math.min(1, dt * 12);
    probeUniforms.uOpacity.value += (probeOpacity - probeUniforms.uOpacity.value) * Math.min(1, dt * 16);
    presentUniforms.uOpacity.value = presentOpacity;
    presentUniforms.uReveal.value = presentReveal;
    present.visible = presentOpacity > 0.001;
    wallUniforms.uFlatten.value = Math.max(flatten, blendT * 0.9);

    // Power: Select on the handheld switches it off, and it boots again.
    if (rebootAt > 0 && time >= rebootAt) {
      rebootAt = -1;
      powerTarget = 1;
      state = stateWith({ ...GOLDEN_STATE, checks: [...GOLDEN_STATE.checks], focus: 2 });
      surface.fill(0);
      for (let i = 0; i < CELLS; i++) surface[i * 4 + 3] = 255;
      colorTexture.needsUpdate = true;
      handheldDraw(state);
    }
    power += (powerTarget - power) * Math.min(1, dt * (powerTarget < power ? 3.2 : 1.6));
    compositeUniforms.uPower.value = handheldMix > 0.5 ? power : 1;

    // Poses: the wall on its own, or as the handheld's screen.
    handheld.group.visible = blendT > 0.001;
    handheld.group.position.copy(handheldPose.position);
    handheld.group.quaternion.copy(handheldPose.quaternion);
    const bodyScale = handheldPose.scale * mix(0.82, 1, blendT);
    handheld.group.scale.setScalar(bodyScale);
    handheld.group.position.y += (1 - blendT) * handheldPose.scale * 40;
    for (const material of handheld.group.userData.materials) {
      material.transparent = blendT < 0.999;
      material.opacity = blendT;
    }
    handheld.group.updateMatrixWorld();
    screenOffset.set(0, BODY.screenY, -0.6).applyMatrix4(handheld.group.matrixWorld);
    pose.position.copy(wallPose.position).lerp(screenOffset, blendT);
    pose.quaternion.copy(wallPose.quaternion).slerp(handheldPose.quaternion, blendT);
    pose.scale = mix(wallPose.scale, handheldPose.scale * (92 / (GRID_W + 5)) * (GRID_W + 5) / GRID_W, blendT);
    frame.visible = blendT < 0.98;
    hudMesh.visible = blendT < 0.6;
    reticleMesh.visible = true;
    reticleMesh.position.set(mix(GRID_W / 2 - 1.2, GRID_W / 2 - 2.2, blendT), mix(GRID_H / 2 + 3.9, -GRID_H / 2 - 3.2, blendT), 0.3);
    reticleUniforms.uOpacity.value = mix(1, 0.8, blendT);
    wallGroup.position.copy(pose.position);
    wallGroup.quaternion.copy(pose.quaternion);
    wallGroup.scale.setScalar(pose.scale);
    // Buttons spring back after a press; the pad rocks towards the direction pressed.
    for (const button of handheld.buttons) {
      const data = button.userData;
      data.press = Math.max(0, data.press - dt * 6);
      if (data.travel) button.position.z = data.rest - data.travel * smooth(Math.min(1, data.press));
    }
    const padPress = handheld.group.userData.padPress;
    if (padPress) {
      padPress.t = Math.max(0, padPress.t - dt * 6);
      const tilt = 0.18 * smooth(padPress.t);
      const pad = handheld.group.userData.pad;
      pad.rotation.set(padPress.action === 'up' ? -tilt : padPress.action === 'down' ? tilt : 0, padPress.action === 'left' ? -tilt : padPress.action === 'right' ? tilt : 0, 0);
    }
    const drawing = step && step.kind === 'draw';
    handheld.group.userData.led.material.color.setRGB(drawing ? 0.2 : 0.05, drawing ? 1.6 : 0.3, drawing ? 0.5 : 0.12);

    // The lamp: the pointer while it moves over the hero, a slow drift otherwise.
    const idle = !pointerActive || performance.now() - lastPointer > 4000;
    wallGroup.updateMatrixWorld();
    const scaleNow = pose.scale;
    if (idle) {
      tmp.set(Math.sin(time * 0.37) * GRID_W * 0.35, Math.cos(time * 0.23) * GRID_H * 0.3, 26);
      lampTarget.copy(tmp).applyMatrix4(wallGroup.matrixWorld);
      hoverStrength += (0 - hoverStrength) * Math.min(1, dt * 3);
    } else {
      raycaster.setFromCamera(pointer, camera);
      inverse.copy(wallGroup.matrixWorld).invert();
      const localRay = raycaster.ray.clone().applyMatrix4(inverse);
      const hit = localRay.intersectPlane(zPlane, tmp);
      if (hit) {
        hoverCell.set(hit.x + GRID_W / 2, GRID_H / 2 - hit.y);
        const over = Math.abs(hit.x) < GRID_W / 2 + 4 && Math.abs(hit.y) < GRID_H / 2 + 4;
        hoverStrength += ((over && handheldMix < 0.5 ? 1 : 0) - hoverStrength) * Math.min(1, dt * 6);
        tmp.set(hit.x, hit.y, 22);
        lampTarget.copy(tmp).applyMatrix4(wallGroup.matrixWorld);
      }
    }
    presence += ((idle ? 0.3 : 1) - presence) * (reduceMotion ? 1 : Math.min(1, dt * 3));
    lampPosition.lerp(lampTarget, reduceMotion ? 1 : Math.min(1, dt * 7));
    wallUniforms.uLampPos.value.copy(lampPosition);
    wallUniforms.uLamp.value = presence;
    wallUniforms.uReach.value = scaleNow * 26;
    wallUniforms.uHover.value.set(hoverCell.x, hoverCell.y, hoverStrength);
    lamp.position.copy(lampPosition);
    lamp.intensity = presence * handheldPose.scale * handheldPose.scale * 1800 * blendT;

    // Render: scene, bloom, composite.
    renderer.setRenderTarget(sceneTarget);
    renderer.setClearColor(0x000000, 1);
    renderer.clear();
    renderer.render(scene, camera);
    pass(brightMaterial, bloomTargets[0]);
    blur(bloomTargets[0], bloomTargets[1], 1.0);
    blur(bloomTargets[0], bloomTargets[1], 2.0);
    copyMaterial.uniforms.tInput.value = bloomTargets[0].texture;
    pass(copyMaterial, bloomTargets[2]);
    blur(bloomTargets[2], bloomTargets[3], 1.5);
    blur(bloomTargets[2], bloomTargets[3], 3.0);
    pass(compositeMaterial, null);

    if (first) {
      first = false;
      root.classList.add('is-live');
      // Compile the handheld's shaders while nobody needs them, so the secret opens without a stall.
      if (!handheld.group.visible && renderer.compileAsync) {
        handheld.group.visible = true;
        renderer.compileAsync(handheld.group, camera, scene).catch(() => {}).finally(() => {
          handheld.group.visible = handheldMix > 0.001;
        });
        handheld.group.visible = false;
      }
    }
    const settling = handheldMix !== handheldTarget || Math.abs(power - powerTarget) > 0.001;
    if (visible && !document.hidden && (!reduceMotion || settling)) requestFrame();
  }

  function mix(a, b, t) {
    return a + (b - a) * t;
  }

  function requestFrame() {
    if (running || lost) return;
    running = true;
    requestAnimationFrame(frameTick);
  }

  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault();
    lost = true;
    root.classList.remove('is-live');
  });
  canvas.addEventListener('webglcontextrestored', () => {
    canvas.remove();
    root.classList.remove('is-live', 'is-handheld');
    mount(root);
  });

  layout();
  checkAspect();
  new ResizeObserver(() => {
    layout();
    requestFrame();
  }).observe(root);
  addEventListener('resize', () => {
    checkAspect();
    requestFrame();
  }, { passive: true });
  if (screen.orientation) screen.orientation.addEventListener('change', checkAspect);
  if (document.fonts) {
    document.fonts.ready.then(() => {
      layout();
      requestFrame();
    });
  }
  new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
    if (visible) {
      clock.getDelta();
      requestFrame();
    }
  }).observe(root);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && visible) {
      clock.getDelta();
      requestFrame();
    }
  });
}

for (const root of document.querySelectorAll('[data-dw-hero-art]')) mount(root);
