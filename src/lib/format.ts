const whole = new Intl.NumberFormat("en-US");
const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
const clockFmt = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });
const dayYearFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "long", year: "numeric" });
const shortFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const shortYearFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short", year: "numeric" });
const monthFmt = new Intl.DateTimeFormat(undefined, { month: "short" });
const monthYearFmt = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });

export function num(n: number): string {
  return whole.format(n);
}

/** 17,122 stays whole; 129,000 becomes 129K. */
export function short(n: number): string {
  return Math.abs(n) < 100000 ? whole.format(n) : compact.format(n);
}

export function signed(n: number): string {
  return n > 0 ? `+${whole.format(n)}` : n < 0 ? `−${whole.format(-n)}` : "0";
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${whole.format(n)} ${n === 1 ? one : many}`;
}

/** The local calendar day of a timestamp, as YYYY-MM-DD. */
export function localDay(at: Date | string): string {
  const d = typeof at === "string" ? new Date(at) : at;
  const pad = (v: number) => String(v).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Parses YYYY-MM-DD as a local date. */
export function parseDay(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(day: string, n: number): string {
  const d = parseDay(day);
  d.setDate(d.getDate() + n);
  return localDay(d);
}

export function clock(iso: string): string {
  return clockFmt.format(new Date(iso));
}

export function dayHeading(day: string, now = new Date()): string {
  if (day === localDay(now)) return "Today";
  if (day === addDays(localDay(now), -1)) return "Yesterday";
  const d = parseDay(day);
  return d.getFullYear() === now.getFullYear() ? dayFmt.format(d) : dayYearFmt.format(d);
}

export function shortDate(at: string, now = new Date()): string {
  const d = at.length === 10 ? parseDay(at) : new Date(at);
  return d.getFullYear() === now.getFullYear() ? shortFmt.format(d) : shortYearFmt.format(d);
}

export function monthName(day: string): string {
  return monthFmt.format(parseDay(day));
}

/** A day's month and year, such as March 2021. */
export function monthYear(day: string): string {
  return monthYearFmt.format(parseDay(day));
}

/** A date written out in full, with its year. */
export function fullDate(at: Date): string {
  return dayYearFmt.format(at);
}

export function ago(iso: string | null, now = Date.now()): string {
  if (!iso) return "never";
  const secs = Math.max(0, Math.round((now - new Date(iso).getTime()) / 1000));
  if (secs < 45) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return plural(days, "day") + " ago";
  return shortDate(iso);
}

export function until(iso: string | null, now = Date.now()): string | null {
  if (!iso) return null;
  const secs = Math.round((new Date(iso).getTime() - now) / 1000);
  if (secs <= 0) return null;
  if (secs < 60) return "in under a minute";
  const mins = Math.round(secs / 60);
  return mins < 60 ? `in ${mins} min` : `in ${Math.round(mins / 60)} h`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** Round axis steps: 1, 2, 5, 10, 20, 50 and so on. */
export function niceTicks(max: number, count = 4): number[] {
  if (max <= 0) return [0, 1];
  const rough = max / count;
  const pow = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 5, 10].map((m) => m * pow).find((s) => s >= rough) ?? 10 * pow;
  const whole = Math.max(1, Math.round(step));
  const ticks = [];
  for (let v = 0; v < max + whole; v += whole) ticks.push(v);
  return ticks;
}
