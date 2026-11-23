// 핵심 숫자 5칸.
import type { Kpis } from "../api";
import { delta, num, pct, ratio } from "../lib/format";

export function KpiTiles({ kpis, exclude404 }: { kpis: Kpis; exclude404: boolean }) {
  const { cur, prev, foreignIps404, newCountries } = kpis;
  // 404를 낸 IP의 비율은 스위치를 끈 상태의 해외 고유 IP를 분모로 한다(켜면 cur.foreignIps에서 이미 빠져 있음).
  const foreignIpsAll = cur.foreignIps + (exclude404 ? foreignIps404 : 0);
  const tiles: { title: string; value: string; sub: string }[] = [
    { title: "전체 요청", value: num(cur.totalRequests), sub: `직전 기간 대비 ${delta(cur.totalRequests, prev.totalRequests)}` },
    { title: "해외 요청", value: num(cur.foreignRequests), sub: `전체의 ${pct(ratio(cur.foreignRequests, cur.totalRequests))}` },
    { title: "해외 고유 IP", value: num(cur.foreignIps), sub: `국내 고유 IP ${num(cur.domesticIps)}` },
    { title: "그중 404를 낸 IP", value: num(foreignIps404), sub: `해외 IP의 ${pct(ratio(foreignIps404, foreignIpsAll))}` },
    { title: "접속 국가 수", value: num(cur.countries), sub: `신규 국가 ${num(newCountries)}` },
  ];
  return (
    <div className="kpis">
      {tiles.map((t) => (
        <div className="kpi" key={t.title}>
          <small>{t.title}</small>
          <b>{t.value}</b>
          <small>{t.sub}</small>
        </div>
      ))}
    </div>
  );
}
