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

/** 기간 지정에서 받는 가장 이른 날짜 */
export const MIN_DATE = "2000-01-01";
/** 기간 지정의 최대 일수(양끝 포함) */
export const MAX_PERIOD_DAYS = 366;

/** "YYYY-MM-DD"이고 달력에 있는 날짜인지 */
function isDate(s: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(s) && Number.isFinite(toMs(s)) && fromMs(toMs(s)) === s;
}

/** ok=false이고 error=null이면 아직 입력 중(빈 칸 등)이라 조용히 무시한다. */
export type PeriodCheck = { ok: true; period: Period } | { ok: false; error: string | null };

/** 기간 지정 입력 검사. 통과하지 못하면 조회하지 않는다. */
export function checkCustomPeriod(from: string, to: string): PeriodCheck {
  if (!isDate(from) || !isDate(to)) return { ok: false, error: null };
  if (from < MIN_DATE || to < MIN_DATE) return { ok: false, error: `${MIN_DATE} 이후 날짜를 지정해 주세요.` };
  if (from > to) return { ok: false, error: "시작일이 종료일보다 늦습니다." };
  if (periodDays({ from, to }) > MAX_PERIOD_DAYS) {
    return { ok: false, error: `기간은 최대 ${MAX_PERIOD_DAYS}일까지 지정할 수 있습니다.` };
  }
  return { ok: true, period: { from, to } };
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
