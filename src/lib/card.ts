import wideCut from "@fontsource-variable/mona-sans/files/mona-sans-latin-wdth-normal.woff2?url";
import { encode } from "uqr";
import { fullDate, monthYear, num, parseDay, signed } from "./format";
import type { Point, RepoDetail } from "./types";

/** A card on sage paper, or on the hedge green of the app's icon. */
export type Look = "light" | "dark";

/** The card's size in its own units, and how many pixels each unit gets in the saved picture. */
export const CARD = { width: 1080, height: 1350, scale: 2 };

interface Fact {
  value: string;
  label: string;
}

export interface CardData {
  owner: string | null;
  name: string;
  description: string | null;
  language: string | null;
  /** Where the QR code leads. */
  url: string;
  stars: number;
  /** Running totals, one point per day the total changed. */
  series: Point[];
  /** Up to three other figures worth showing off, the best first. */
  facts: Fact[];
  counted: Date;
}

/** Gathers what a repository's card says. Figures that are zero are left off. */
export function cardData(detail: RepoDetail, counted = new Date()): CardData {
  const { repo } = detail;
  const slash = repo.name.indexOf("/");
  const facts: Fact[] = [];
  const add = (n: number, value: string, one: string, many: string) => {
    if (n > 0) facts.push({ value, label: n === 1 ? one : many });
  };
  add(repo.stars_30d, signed(repo.stars_30d), "star in 30 days", "stars in 30 days");
  add(repo.forks, num(repo.forks), "fork", "forks");
  if (repo.release_count) add(repo.downloads, num(repo.downloads), "download", "downloads");
  add(repo.watchers, num(repo.watchers), "watcher", "watchers");
  facts.length = Math.min(facts.length, 3);
  return {
    owner: slash > 0 ? repo.name.slice(0, slash) : null,
    name: repo.name.slice(slash + 1),
    description: repo.description?.trim() || null,
    language: repo.language,
    // Sample repositories have no address, so theirs is worked out from the name.
    url: repo.url || `https://github.com/${repo.name}`,
    stars: repo.stars,
    series: detail.star_series,
    facts,
    counted,
  };
}

type Ctx = CanvasRenderingContext2D;

const W = CARD.width;
const H = CARD.height;
/** The margin every edge keeps. */
const EDGE = 72;
/** How high the hill climbs, and where its foot stands. */
const PEAK = 452;
const GROUND = 930;
/** Where the history ends and the level ground the bird stands on begins. */
const LEDGE = 800;
/** How wide a rim the hill leaves around the count, as a share of the count's size. */
const RIM = 0.055;
/** How wide text can run on the lines the bird stands beside. */
const BESIDE_BIRD = 720;
/** How wide the QR code is. */
const QR = 288;
/** How finely the history is read across the hill. */
const COLUMN = 8;
/** The most stars a hill can hold and still climb a stair for each. More are drawn as a slope. */
const MOST_STAIRS = 40;
/** A day's gain this large, as a share of the highest total, stays a cliff on a smooth hill. */
const CLIFF = 0.15;

interface Colours {
  sky: string;
  ink: string;
  soft: string;
  hill: string;
  onHill: string;
  onHillSoft: string;
  /** Whether the sky's ink would be lost against the hill, so the hill must stand back from it. */
  standsBack: boolean;
}

const COLOURS: Record<Look, Colours> = {
  light: {
    sky: "#eef2e9",
    ink: "#17211b",
    soft: "#46544a",
    hill: "#e2a51f",
    onHill: "#17211b",
    onHillSoft: "#17211bb8",
    standsBack: false,
  },
  dark: {
    sky: "#234333",
    ink: "#f7f1df",
    soft: "#a9c0ae",
    hill: "#e9b23c",
    onHill: "#13241b",
    onHillSoft: "#13241bb8",
    standsBack: true,
  },
};

const SANS = '"Mona Sans Variable", system-ui, sans-serif';
const MONO = '"Monaspace Neon", ui-monospace, monospace';
/** The wide cut of Mona Sans, which sets the repository's name and every figure. */
const WIDE = "Mona Sans Wide";

const sans = (weight: number, size: number) => `${weight} ${size}px ${SANS}`;
const wide = (weight: number, size: number) => `${weight} ${size}px "${WIDE}", ${SANS}`;
const mono = (size: number) => `500 ${size}px ${MONO}`;

let wideLoaded: Promise<unknown> | undefined;

/**
 * Loads the typefaces the card is set in. A canvas draws with a fallback until they are ready.
 * It cannot ask a variable font for a width either, so the wide cut becomes a family of its own.
 */
export async function loadCardFonts(card: CardData): Promise<void> {
  wideLoaded ??= new FontFace(WIDE, `url(${wideCut})`, { weight: "200 900", stretch: "125%" })
    .load()
    .then((face) => document.fonts.add(face));
  const text = [card.owner, card.description, card.language, "0123456789"].join(" ");
  await Promise.all([
    wideLoaded,
    document.fonts.load(sans(500, 30), text),
    document.fonts.load(mono(24), card.url),
  ]);
}

/** Draws the card at full size on a canvas of its own, ready to be saved. */
export function cardCanvas(card: CardData, look: Look): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = CARD.width * CARD.scale;
  canvas.height = CARD.height * CARD.scale;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("The card could not be drawn.");
  ctx.scale(CARD.scale, CARD.scale);
  drawCard(ctx, card, look);
  return canvas;
}

/** Draws the card onto a canvas already scaled so that the card's own units fill it. */
export function drawCard(ctx: Ctx, card: CardData, look: Look) {
  const c = COLOURS[look];
  ctx.save();
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = c.sky;
  ctx.fillRect(0, 0, W, H);

  const hill = raiseHill(card.series);
  ctx.fillStyle = c.hill;
  ctx.fill(hill.path);

  heading(ctx, card, c);
  count(ctx, card, c, hill.path);
  // With no hill to stand on, the bird keeps clear of the QR code below it.
  bird(ctx, hill.ledge < GROUND ? 928 : 640, hill.ledge, 0.36);
  tally(ctx, card.facts, c);
  code(ctx, card.url, W - EDGE - QR, H - EDGE - QR, QR, c);
  footer(ctx, card, c);
  ctx.restore();
}

/**
 * The star history as a hill that fills the card below it, running on as level ground once it
 * reaches today. A history with few changes climbs in stairs, and a busy one in a smooth slope
 * that keeps its sudden jumps.
 */
function raiseHill(series: Point[]): { path: Path2D; ledge: number } {
  const days = series.map((p) => ({ t: parseDay(p.date).getTime(), value: p.value }));
  const top = Math.max(1, ...days.map((d) => d.value));
  const y = (v: number) => GROUND - (v / top) * (GROUND - PEAK);
  const path = new Path2D();
  path.moveTo(0, H);

  const first = days[0];
  const last = days.at(-1);
  // With one day or none, nothing is known of how the total was reached.
  if (!first || !last || first.t === last.t) {
    const level = y(last?.value ?? 0);
    path.lineTo(0, level);
    return closeHill(path, level);
  }

  const x = (t: number) => ((t - first.t) / (last.t - first.t)) * LEDGE;
  const when = (at: number) => first.t + (at / LEDGE) * (last.t - first.t);
  const totalAt = (t: number) => days.findLast((d) => d.t <= t)?.value ?? first.value;

  const columns = Array.from({ length: LEDGE / COLUMN + 1 }, (_, i) => totalAt(when(i * COLUMN)));

  let level = Math.round(y(first.value));
  path.lineTo(0, level);
  if (top <= MOST_STAIRS) {
    for (const [i, total] of columns.entries()) {
      if (i === 0 || total === columns[i - 1]) continue;
      path.lineTo(i * COLUMN, level);
      level = Math.round(y(total));
      path.lineTo(i * COLUMN, level);
    }
    return closeHill(path, level);
  }

  // Each stretch between two cliffs is a slope of its own.
  let from = { at: 0, total: first.value };
  const slopeTo = (at: number, total: number) => {
    const knots: [number, number][] = [[from.at, y(from.total)]];
    for (let k = Math.ceil(from.at / (2 * COLUMN)) * 2 * COLUMN; k < at; k += 2 * COLUMN) {
      if (k - from.at >= COLUMN && at - k >= COLUMN) knots.push([k, y(totalAt(when(k)))]);
    }
    knots.push([at, y(total)]);
    slope(path, knots);
  };
  for (const [i, day] of days.entries()) {
    const before = days[i - 1];
    if (!before || day.value - before.value < CLIFF * top) continue;
    const at = Math.round(x(day.t));
    if (at > from.at) slopeTo(at, before.value);
    path.lineTo(at, y(day.value));
    from = { at, total: day.value };
  }
  if (from.at < LEDGE) slopeTo(LEDGE, last.value);
  return closeHill(path, y(last.value));
}

/** Runs the hill on as level ground to the right edge, and fills in everything beneath it. */
function closeHill(path: Path2D, level: number): { path: Path2D; ledge: number } {
  path.lineTo(W, level);
  path.lineTo(W, H);
  path.closePath();
  return { path, ledge: level };
}

/**
 * Carries a path on from its first point through the rest in a smooth line that never
 * rises or dips beyond the points themselves.
 */
function slope(path: Path2D, points: [number, number][]) {
  const n = points.length;
  if (n < 3) {
    for (const [px, py] of points.slice(1)) path.lineTo(px, py);
    return;
  }
  const rise = (i: number) => (points[i + 1][1] - points[i][1]) / (points[i + 1][0] - points[i][0]);
  const lean = points.map((_, i) => {
    if (i === 0 || i === n - 1) return 0;
    const [a, b] = [rise(i - 1), rise(i)];
    const [ha, hb] = [points[i][0] - points[i - 1][0], points[i + 1][0] - points[i][0]];
    const mean = (a * hb + b * ha) / (ha + hb);
    return (Math.sign(a) + Math.sign(b)) * Math.min(Math.abs(a), Math.abs(b), Math.abs(mean) / 2);
  });
  lean[0] = (3 * rise(0) - lean[1]) / 2;
  lean[n - 1] = (3 * rise(n - 2) - lean[n - 2]) / 2;
  for (let i = 0; i < n - 1; i++) {
    const [[x0, y0], [x1, y1]] = [points[i], points[i + 1]];
    const third = (x1 - x0) / 3;
    path.bezierCurveTo(x0 + third, y0 + lean[i] * third, x1 - third, y1 - lean[i + 1] * third, x1, y1);
  }
}

/** Who owns the repository, its name, and what it is for. */
function heading(ctx: Ctx, card: CardData, c: Colours) {
  ctx.fillStyle = c.soft;
  let ownerRoom = W - 2 * EDGE;
  if (card.language) {
    ctx.font = sans(560, 28);
    const language = clip(ctx, card.language, 360);
    const mark = W - EDGE - ctx.measureText(language).width - 30;
    ctx.textAlign = "right";
    ctx.fillText(language, W - EDGE, 106);
    ctx.textAlign = "left";
    ctx.fill(box(mark, 106 - 19, 18, 18, 4.5));
    ownerRoom = mark - 48 - EDGE;
  }
  if (card.owner) {
    ctx.font = sans(520, 32);
    const slash = ctx.measureText(" /").width;
    ctx.fillText(`${clip(ctx, card.owner, ownerRoom - slash)} /`, EDGE, 106);
  }

  const title = fitName(ctx, card.name);
  ctx.fillStyle = c.ink;
  ctx.font = wide(800, title.size);
  tracking(ctx, -0.012 * title.size);
  let line = 128 + title.size * 0.74;
  for (const text of title.lines) {
    ctx.fillText(text, EDGE - title.size * 0.035, line);
    line += title.size;
  }
  tracking(ctx, 0);
  line -= title.size;

  if (!card.description) return;
  ctx.fillStyle = c.soft;
  ctx.font = sans(440, 30);
  const [gap, most] = title.lines.length > 1 ? [56, 2] : [64, 3];
  for (const [i, text] of wrap(ctx, card.description, BESIDE_BIRD, most).entries()) {
    ctx.fillText(text, EDGE, line + gap + i * 40);
  }
}

/**
 * Sets the name on one line if it fits at a good size, or else breaks it in two. A short
 * name is set larger, and a second line stops short of the bird.
 */
function fitName(ctx: Ctx, name: string): { lines: string[]; size: number } {
  const full = W - 2 * EDGE;
  const widthAt = (text: string, size: number) => {
    ctx.font = wide(800, size);
    return ctx.measureText(text).width;
  };

  const natural = widthAt(name, 116);
  if (natural <= full * 0.75) return { lines: [name], size: Math.min(140, Math.floor((116 * full * 0.75) / natural)) };
  const whole = Math.max(78, Math.min(116, Math.floor((116 * full) / natural)));
  if (widthAt(name, whole) <= full) return { lines: [name], size: whole };

  // Break after the separator that leaves the two halves most alike.
  let cut = Math.ceil(name.length / 2);
  let best = Infinity;
  for (const m of name.matchAll(/[-_.]/g)) {
    const at = m.index + 1;
    const lopsided = Math.abs(name.length - 2 * at);
    if (at < name.length && lopsided < best) [cut, best] = [at, lopsided];
  }
  const halves = [name.slice(0, cut), name.slice(cut)];
  const rooms = [full, BESIDE_BIRD];
  // Two lines at 80 still end above the bird's head.
  const fits = halves.map((half, i) => Math.floor((80 * rooms[i]) / widthAt(half, 80)));
  const size = Math.max(56, Math.min(80, ...fits));
  ctx.font = wide(800, size);
  return { lines: halves.map((half, i) => clip(ctx, half, rooms[i])), size };
}

/**
 * The star count, large, with a line beneath saying what it counts. Both sit in front of the
 * hill. Where light ink would be lost against it, the hill leaves a rim of sky around the
 * count and a strip of sky behind the line.
 */
function count(ctx: Ctx, card: CardData, c: Colours, hill: Path2D) {
  const from = since(card);
  const noun = card.stars === 1 ? "star" : "stars";
  const [figure, label] =
    card.stars === 0
      ? ["No stars yet", "Be the first"]
      : [num(card.stars), from ? `${noun} since ${from}` : noun];
  ctx.font = wide(800, 200);
  const size = Math.min(200, Math.floor((200 * (W - 2 * EDGE)) / ctx.measureText(figure).width));
  const left = EDGE - size * 0.04;

  ctx.font = wide(800, size);
  tracking(ctx, -0.02 * size);
  if (c.standsBack) {
    ctx.save();
    ctx.clip(hill);
    ctx.strokeStyle = c.sky;
    ctx.lineJoin = "round";
    ctx.lineWidth = 2 * RIM * size;
    ctx.strokeText(figure, left, 742);
    ctx.restore();
  }
  ctx.fillStyle = c.ink;
  ctx.fillText(figure, left, 742);
  tracking(ctx, 0);

  ctx.font = sans(520, 36);
  if (c.standsBack) {
    ctx.save();
    ctx.clip(hill);
    ctx.fillStyle = c.sky;
    ctx.fill(box(EDGE - 14, 816 - 37, ctx.measureText(label).width + 28, 52, 12));
    ctx.restore();
  }
  ctx.fillStyle = c.soft;
  ctx.fillText(label, EDGE, 816);
}

/** The month the history begins. One that opens well above zero is missing its start. */
function since(card: CardData): string | null {
  const first = card.series[0];
  if (!first || first.value > Math.max(3, card.stars * 0.1)) return null;
  return monthYear(first.date);
}

/** The other figures, one to a line down the hill. Fewer of them are set larger. */
function tally(ctx: Ctx, facts: Fact[], c: Colours) {
  const [value, label, pitch] = [[52, 42, 0], [44, 37, 66], [36, 32, 56]][Math.min(facts.length, 3) - 1] ?? [];
  for (const [i, fact] of facts.entries()) {
    // The first line's capitals reach as high as the QR code beside them.
    const line = H - EDGE - QR + value * 0.73 + i * pitch;
    ctx.font = wide(760, value);
    ctx.fillStyle = c.onHill;
    ctx.fillText(fact.value, EDGE, line);
    const after = EDGE + ctx.measureText(fact.value).width + value / 3;
    ctx.font = sans(480, label);
    ctx.fillStyle = c.onHillSoft;
    ctx.fillText(fact.label, after, line);
  }
}

/** The repository's address, the day the figures were read, and what read them. */
function footer(ctx: Ctx, card: CardData, c: Colours) {
  const room = W - 2 * EDGE - QR - 40;
  const address = card.url.replace(/^https?:\/\//, "");
  ctx.font = mono(26);
  const widths = (lines: string[]) => Math.max(...lines.map((l) => ctx.measureText(l).width));
  // An address too long for one line breaks before the repository's name.
  const cut = address.lastIndexOf("/") + 1;
  const lines = widths([address]) > room && cut > 0 ? [address.slice(0, cut), address.slice(cut)] : [address];
  const size = Math.max(19, Math.min(26, Math.floor((26 * room) / widths(lines))));
  ctx.font = mono(size);
  ctx.fillStyle = c.onHill;
  for (const [i, line] of lines.entries()) {
    ctx.fillText(clip(ctx, line, room), EDGE, 1236 - (lines.length - 1 - i) * size * 1.35);
  }

  const counted = `Counted ${fullDate(card.counted)} with `;
  ctx.font = sans(480, 25);
  ctx.fillStyle = c.onHillSoft;
  ctx.fillText(counted, EDGE, 1278);
  const after = EDGE + ctx.measureText(counted).width;
  ctx.font = sans(700, 25);
  ctx.fillStyle = c.onHill;
  ctx.fillText("Birdfeeder", after, 1278);
}

/**
 * A QR code drawn the way the app draws a day of activity: one rounded cell for each module.
 * The cells stay close enough together for a camera to read them.
 */
function code(ctx: Ctx, url: string, x: number, y: number, size: number, c: Colours) {
  const { size: n, data } = encode(url, { ecc: "Q", border: 0 });
  const cell = size / n;
  const gap = cell * 0.14;
  const corners = [
    [0, 0],
    [n - 7, 0],
    [0, n - 7],
  ];
  const inCorner = (col: number, row: number) =>
    corners.some(([cx, cy]) => col >= cx && col < cx + 7 && row >= cy && row < cy + 7);

  const cells = new Path2D();
  for (let row = 0; row < n; row++) {
    for (let col = 0; col < n; col++) {
      if (!data[row][col] || inCorner(col, row)) continue;
      cells.addPath(box(x + col * cell + gap / 2, y + row * cell + gap / 2, cell - gap, cell - gap, cell * 0.24));
    }
  }
  ctx.fillStyle = c.onHill;
  ctx.fill(cells);

  // The corner markers keep their true proportions, which is what a camera looks for first.
  for (const [cx, cy] of corners) {
    const square = (inset: number, colour: string) => {
      const across = (7 - 2 * inset) * cell;
      ctx.fillStyle = colour;
      ctx.fill(box(x + (cx + inset) * cell, y + (cy + inset) * cell, across, across, cell * 0.24));
    };
    square(0, c.onHill);
    square(1, c.hill);
    square(2, c.onHill);
  }
}

/** The bird from the app's icon, standing with its feet at the given point. */
function bird(ctx: Ctx, x: number, ground: number, size: number) {
  ctx.save();
  ctx.translate(x - 507 * size, ground - 763 * size);
  ctx.scale(size, size);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  ctx.strokeStyle = "#ee8140";
  ctx.lineWidth = 22;
  ctx.beginPath();
  ctx.moveTo(458, 700);
  ctx.lineTo(458, 752);
  ctx.moveTo(556, 700);
  ctx.lineTo(556, 752);
  ctx.stroke();

  const tail = new Path2D("M348 500 L348 592 L214 470 L262 400 Z");
  ctx.fillStyle = ctx.strokeStyle = "#d2921c";
  ctx.lineWidth = 44;
  ctx.fill(tail);
  ctx.stroke(tail);

  ctx.fillStyle = "#f4bc3e";
  ctx.fill(box(318, 326, 388, 388, 110));
  ctx.fillStyle = "#13241b";
  ctx.fill(new Path2D("M572 500 C486 486 402 532 366 618 C446 660 552 650 580 574 C590 546 586 520 572 500 Z"));

  const beak = new Path2D("M702 442 L792 478 L702 514 Z");
  ctx.fillStyle = ctx.strokeStyle = "#ee8140";
  ctx.lineWidth = 18;
  ctx.fill(beak);
  ctx.stroke(beak);

  ctx.fillStyle = "#13241b";
  ctx.beginPath();
  ctx.arc(614, 450, 26, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#fff6dd";
  ctx.beginPath();
  ctx.arc(622, 441, 8, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** A square with rounded corners. */
function box(x: number, y: number, w: number, h: number, r: number): Path2D {
  const path = new Path2D();
  path.moveTo(x + r, y);
  path.arcTo(x + w, y, x + w, y + h, r);
  path.arcTo(x + w, y + h, x, y + h, r);
  path.arcTo(x, y + h, x, y, r);
  path.arcTo(x, y, x + w, y, r);
  path.closePath();
  return path;
}

/** Tightens or loosens the letters, where the canvas can. */
function tracking(ctx: Ctx, px: number) {
  if ("letterSpacing" in ctx) ctx.letterSpacing = `${px.toFixed(2)}px`;
}

/** Shortens text to a width, ending it with an ellipsis. */
function clip(ctx: Ctx, text: string, width: number): string {
  if (ctx.measureText(text).width <= width) return text;
  let end = text.length;
  while (end > 1 && ctx.measureText(`${text.slice(0, end).trimEnd()}…`).width > width) end -= 1;
  return `${text.slice(0, end).trimEnd()}…`;
}

/** Breaks text into lines of a width. Past the last line allowed it ends at a whole word, with an ellipsis. */
function wrap(ctx: Ctx, text: string, width: number, most: number): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/)) {
    const longer = line ? `${line} ${word}` : word;
    if (!line || ctx.measureText(longer).width <= width) {
      line = longer;
    } else {
      lines.push(line);
      line = word;
    }
  }
  lines.push(line);
  if (lines.length > most) {
    lines.length = most;
    const words = lines[most - 1].split(" ");
    const ended = () => `${words.join(" ").replace(/[.,;:!?]+$/, "")}…`;
    while (words.length > 1 && ctx.measureText(ended()).width > width) words.pop();
    lines[most - 1] = ended();
  }
  return lines.map((l) => clip(ctx, l, width));
}
