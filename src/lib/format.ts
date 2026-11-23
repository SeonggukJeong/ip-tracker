// 화면 숫자·날짜 표기. 어떤 입력이 와도 NaN·Infinity를 내보내지 않는다.

const KST_S = 9 * 3600;
const numFmt = new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 0 });

const pad2 = (n: number) => String(n).padStart(2, "0");

/** 4812330 → "4,812,330" */
export function num(n: number): string {
  return numFmt.format(Number.isFinite(n) ? n : 0);
}

/** 0.0204 → "2.0%", NaN/Infinity → "0.0%" */
export function pct(ratio: number, digits = 1): string {
  return `${((Number.isFinite(ratio) ? ratio : 0) * 100).toFixed(digits)}%`;
}

/** 나눗셈을 안전하게. 분모가 0이거나 결과가 유한하지 않으면 0 */
export function ratio(a: number, b: number): number {
  const r = a / b;
  return b !== 0 && Number.isFinite(r) ? r : 0;
}

/** "+3.1%", "-2.0%", prev가 0이면 "–" */
export function delta(cur: number, prev: number): string {
  if (!Number.isFinite(cur) || !Number.isFinite(prev) || prev === 0) return "–";
  const r = ((cur - prev) / prev) * 100;
  const s = Math.abs(r).toFixed(1);
  if (s === "0.0") return "0.0%";
  return `${r > 0 ? "+" : "-"}${s}%`;
}

/** KST 기준 날짜 부분 */
function kstParts(ts: number) {
  const d = new Date(((Number.isFinite(ts) ? ts : 0) + KST_S) * 1000);
  return {
    y: d.getUTCFullYear(),
    mo: d.getUTCMonth() + 1,
    d: d.getUTCDate(),
    h: pad2(d.getUTCHours()),
    mi: pad2(d.getUTCMinutes()),
    s: pad2(d.getUTCSeconds()),
  };
}

/** 유닉스 초 → "11/22 23:58" (KST) */
export function dateTime(ts: number): string {
  const p = kstParts(ts);
  return `${p.mo}/${p.d} ${p.h}:${p.mi}`;
}

/** 유닉스 초 → "2026-11-22 23:58:07" (KST). 개별 요청 목록용 */
export function dateTimeFull(ts: number): string {
  const p = kstParts(ts);
  return `${p.y}-${pad2(p.mo)}-${pad2(p.d)} ${p.h}:${p.mi}:${p.s}`;
}

/** "2026-11-22" → "11/22" */
export function monthDay(date: string): string {
  const [, m, d] = date.split("-").map(Number);
  return Number.isFinite(m) && Number.isFinite(d) ? `${m}/${d}` : date;
}
