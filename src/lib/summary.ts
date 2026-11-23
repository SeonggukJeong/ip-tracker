// 보고서 문구: 제목, 요약 문단, 기간 표기, 하단 안내. 숫자는 format의 함수로만 찍어 NaN이 나가지 않게 한다.
import type { Period } from "../api";
import { num, pct } from "./format";
import { periodDays } from "./period";

/** 요약 문단. top은 해외 요청이 많은 순(최대 3개 사용), share는 해외 요청 중 비율 0~1 */
export function summarySentence(a: {
  days: number;
  countries: number;
  foreignIps: number;
  foreignShare: number;
  top: { name: string; share: number }[];
}): string {
  const days = num(a.days);
  const top = a.top.slice(0, 3);
  if (!(a.foreignIps > 0) || top.length === 0) return `지난 ${days}일간 해외 접속이 없었습니다.`;
  const names = top.map((t) => t.name).join("·");
  const sum = top.reduce((s, t) => s + (Number.isFinite(t.share) ? t.share : 0), 0);
  return (
    `지난 ${days}일간 ${num(a.countries)}개국 ${num(a.foreignIps)}개 IP가 해외에서 접속했습니다. ` +
    `전체 요청의 ${pct(a.foreignShare)}이며, 상위 ${top.length}개국(${names})이 해외 요청의 ${pct(sum)}를 차지합니다.`
  );
}

/** "" → "해외 접속 현황", "민원24" → "민원24 해외 접속 현황" */
export function reportTitle(serviceName: string): string {
  const s = serviceName.trim();
  return s ? `${s} 해외 접속 현황` : "해외 접속 현황";
}

/** 하단 안내. 404 제외 문구는 스위치가 켜졌을 때만 */
export function reportFootnotes(a: { exclude404: boolean; excludedIps: number; geoBuildDate: string }): string[] {
  const notes = ["국가는 접속 IP 주소 기준으로 판별했으며, VPN 등으로 실제 위치와 다를 수 있습니다."];
  if (a.exclude404) notes.push(`404 응답을 받은 IP ${num(a.excludedIps)}개는 집계에서 제외했습니다.`);
  notes.push(`IP 위치 정보: DB-IP (db-ip.com), CC BY 4.0, ${a.geoBuildDate} 기준`);
  return notes;
}

function ymd(date: string): [number, number, number] | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** "2026. 11. 16. ~ 11. 22. (7일)". 해가 다르면 양쪽에 연도, 하루면 "2026. 11. 22. (1일)" */
export function periodLabel(p: Period): string {
  const a = ymd(p.from);
  const b = ymd(p.to);
  const days = periodDays(p);
  if (!a || !b || days === 0) return `${p.from} ~ ${p.to}`;
  const from = `${a[0]}. ${a[1]}. ${a[2]}.`;
  if (days === 1) return `${from} (1일)`;
  const to = a[0] === b[0] ? `${b[1]}. ${b[2]}.` : `${b[0]}. ${b[1]}. ${b[2]}.`;
  return `${from} ~ ${to} (${num(days)}일)`;
}
