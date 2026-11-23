// 기간 계산. 날짜는 모두 "YYYY-MM-DD"(KST 달력 날짜)이고, 계산은 UTC 자정으로 바꿔서 해 시간대 영향을 받지 않게 한다.
import type { Period } from "../api";

export type Preset = "latest" | "7d" | "30d" | "custom";

/** App이 들고 대시보드·보고서에 넘기는 조회 조건 */
export type ViewFilter = { preset: Preset; period: Period; exclude404: boolean };

const DAY_MS = 86_400_000;
const KST_MS = 9 * 3_600_000;

function toMs(date: string): number {
  return Date.parse(`${date}T00:00:00Z`);
}

function fromMs(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

export function addDays(date: string, days: number): string {
  return fromMs(toMs(date) + days * DAY_MS);
}

/** 오늘 날짜(KST) */
export function todayKst(): string {
  return fromMs(Date.now() + KST_MS);
}

/** latestDate가 null이면 today 기준 */
export function presetPeriod(preset: Exclude<Preset, "custom">, latestDate: string | null, today: string): Period {
  const to = latestDate ?? today;
  const days = preset === "latest" ? 1 : preset === "7d" ? 7 : 30;
  return { from: addDays(to, -(days - 1)), to };
}

/** 양끝 포함 일수. 날짜가 잘못됐거나 from > to면 0 */
export function periodDays(p: Period): number {
  const d = (toMs(p.to) - toMs(p.from)) / DAY_MS + 1;
  return Number.isFinite(d) && d > 0 ? Math.round(d) : 0;
}

/** date에서 months개월 전 날짜. 그 달에 같은 날이 없으면 말일로 맞춘다(8/31 − 6개월 → 2/28 또는 2/29) */
export function monthsBefore(date: string, months: number): string {
  const [y, m, d] = date.split("-").map(Number);
  const idx = y * 12 + (m - 1) - months;
  const ty = Math.floor(idx / 12);
  const tm = idx - ty * 12; // 0~11
  const last = new Date(Date.UTC(ty, tm + 1, 0)).getUTCDate();
  return fromMs(Date.UTC(ty, tm, Math.min(d, last)));
}
