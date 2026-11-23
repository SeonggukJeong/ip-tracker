// 보고서: A4 세로 한 장 미리보기와 인쇄. 대시보드의 현재 기간·404 제외 상태를 그대로 쓴다.
// 구성(목업 02 ②): 제목 줄 → 요약 문단 → 숫자 카드 4개 → 세계지도 + 상위 5개국 → 일별 추이 → 하단 안내.
import { useEffect, useMemo, useState } from "react";
import {
  type AppInfo,
  type CountryRow,
  type DayPoint,
  type Filter,
  getAppInfo,
  getCountries,
  getDaily,
  getKpis,
  getSettings,
  type Kpis,
} from "../api";
import { DailyTrend } from "../components/DailyTrend";
import { TopCountries } from "../components/TopCountries";
import { WorldMap } from "../components/WorldMap";
import { countryName } from "../lib/countryName";
import { delta, num, pct, ratio } from "../lib/format";
import { periodDays, type ViewFilter } from "../lib/period";
import { periodLabel, reportFootnotes, reportTitle, summarySentence } from "../lib/summary";
import "../report.css";

type Data = { kpis: Kpis; countries: CountryRow[]; daily: DayPoint[]; serviceName: string; info: AppInfo };

const EMPTY = "해당 기간에 해외 접속이 없습니다";

export function Report({ filter, onBack }: { filter: ViewFilter; onBack: () => void }) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  const { from, to } = filter.period;
  const { exclude404 } = filter;
  const query: Filter = useMemo(() => ({ period: { from, to }, exclude404 }), [from, to, exclude404]);

  useEffect(() => {
    let live = true;
    setData(null);
    setError(null);
    Promise.all([getKpis(query), getCountries(query), getDaily(query), getSettings(), getAppInfo()]).then(
      ([kpis, countries, daily, settings, info]) =>
        live && setData({ kpis, countries, daily, serviceName: settings.serviceName, info }),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [query]);

  // 차트는 animation: false라 setOption 때 바로 그려진다. 차트 effect가 끝난 다음 프레임에 인쇄 버튼을 켠다.
  useEffect(() => {
    setReady(false);
    if (data === null) return;
    const id = requestAnimationFrame(() => setReady(true));
    return () => cancelAnimationFrame(id);
  }, [data]);

  const top5 = useMemo(
    () => (data ? [...data.countries].sort((a, b) => b.requests - a.requests).slice(0, 5) : []),
    [data],
  );

  return (
    <div className="report-screen">
      <div className="report-toolbar">
        <button type="button" className="btn" onClick={onBack}>
          돌아가기
        </button>
        <button type="button" className="btn primary" disabled={!ready} onClick={() => window.print()}>
          인쇄 / PDF 저장
        </button>
        <span className="muted">인쇄 창에서 프린터를 "Microsoft Print to PDF"로 고르면 PDF 파일로 저장됩니다.</span>
      </div>

      {error && <p className="error">보고서 자료를 불러오지 못했습니다: {error}</p>}
      {data === null ? (
        !error && <p className="muted">불러오는 중…</p>
      ) : (
        <div className="report-scroll">
          <ReportSheet data={data} filter={filter} top5={top5} />
        </div>
      )}
    </div>
  );
}

function ReportSheet({ data, filter, top5 }: { data: Data; filter: ViewFilter; top5: CountryRow[] }) {
  const { cur, prev, newCountries, excludedIps } = data.kpis;
  const share = ratio(cur.foreignRequests, cur.totalRequests);
  const empty = data.countries.length === 0;

  const lead = summarySentence({
    days: periodDays(filter.period),
    countries: cur.countries,
    foreignIps: cur.foreignIps,
    foreignShare: share,
    top: top5.slice(0, 3).map((r) => ({ name: countryName(r.country), share: r.share })),
  });

  const cards = [
    {
      title: "해외 요청",
      value: num(cur.foreignRequests),
      sub: `직전 기간 대비 ${delta(cur.foreignRequests, prev.foreignRequests)}`,
    },
    {
      title: "해외 접속 IP",
      value: num(cur.foreignIps),
      sub: `직전 기간 대비 ${delta(cur.foreignIps, prev.foreignIps)}`,
    },
    { title: "접속 국가", value: num(cur.countries), sub: `신규 ${num(newCountries)}개국` },
    { title: "해외 비중", value: pct(share), sub: `직전 기간 ${pct(ratio(prev.foreignRequests, prev.totalRequests))}` },
  ];

  const notes = reportFootnotes({ exclude404: filter.exclude404, excludedIps, geoBuildDate: data.info.geoBuildDate });

  return (
    <article className="report">
      <header className="rp-top">
        <h1>{reportTitle(data.serviceName)}</h1>
        <span>{periodLabel(filter.period)}</span>
      </header>

      <p className="rp-lead">{lead}</p>

      <div className="rp-cards">
        {cards.map((c) => (
          <div className="rp-card" key={c.title}>
            <small>{c.title}</small>
            <b>{c.value}</b>
            <em>{c.sub}</em>
          </div>
        ))}
      </div>

      <div className="rp-two">
        <section>
          <h2>국가별 분포</h2>
          <div className="rp-chart rp-pair">
            {empty ? (
              <div className="rp-empty">{EMPTY}</div>
            ) : (
              <WorldMap rows={data.countries} height="100%" renderer="svg" animation={false} />
            )}
          </div>
        </section>
        <section>
          <h2>상위 5개국 (요청 수)</h2>
          <div className="rp-chart rp-pair">
            {empty ? (
              <div className="rp-empty">{EMPTY}</div>
            ) : (
              <TopCountries rows={top5} top={5} height="100%" renderer="svg" animation={false} />
            )}
          </div>
        </section>
      </div>

      <section>
        <h2>일별 해외 요청 추이</h2>
        <div className="rp-chart rp-trend">
          <DailyTrend points={data.daily} series={["requests"]} height="100%" renderer="svg" animation={false} />
        </div>
      </section>

      <footer className="rp-foot">
        {notes.map((n) => (
          <p key={n}>{n}</p>
        ))}
      </footer>
    </article>
  );
}
