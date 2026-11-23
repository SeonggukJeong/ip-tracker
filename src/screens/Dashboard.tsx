// 대시보드: 상단 줄 → 제외 스위치 → 핵심 숫자 → 지도·상위 국가 → 일별 추이 → 국가별 상세 → 안내사항.
import { useEffect, useMemo, useState } from "react";
import { type CountryRow, type DayPoint, type Filter, getCountries, getDaily, getKpis, type IngestStatus, type Kpis } from "../api";
import { CountryTable } from "../components/CountryTable";
import { DailyTrend } from "../components/DailyTrend";
import { IpDrawer } from "../components/IpDrawer";
import { KpiTiles } from "../components/KpiTiles";
import { Notices } from "../components/Notices";
import { TopCountries } from "../components/TopCountries";
import { WorldMap } from "../components/WorldMap";
import { monthDay, num } from "../lib/format";
import { type Preset, presetPeriod, todayKst, type ViewFilter } from "../lib/period";

const PRESETS: { key: Preset; label: string }[] = [
  { key: "latest", label: "최근 적재일" },
  { key: "7d", label: "최근 7일" },
  { key: "30d", label: "최근 30일" },
  { key: "custom", label: "기간 지정" },
];

const EMPTY = "해당 기간에 데이터가 없습니다";

type Data = { kpis: Kpis; countries: CountryRow[]; daily: DayPoint[] };

type Props = {
  filter: ViewFilter;
  onFilterChange: (f: ViewFilter) => void;
  status: IngestStatus;
  expectedServers: number;
  refreshKey: number;
  onOpenIngest: () => void;
  onOpenReport: () => void;
};

export function Dashboard({ filter, onFilterChange, status, expectedServers, refreshKey, onOpenIngest, onOpenReport }: Props) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const { from, to } = filter.period;
  const { exclude404 } = filter;
  const query: Filter = useMemo(() => ({ period: { from, to }, exclude404 }), [from, to, exclude404]);

  useEffect(() => {
    let live = true;
    setError(null);
    Promise.all([getKpis(query), getCountries(query), getDaily(query)]).then(
      ([kpis, countries, daily]) => live && setData({ kpis, countries, daily }),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [query, refreshKey]);

  const choosePreset = (preset: Preset) => {
    const period = preset === "custom" ? filter.period : presetPeriod(preset, status.latestDate, todayKst());
    onFilterChange({ ...filter, preset, period });
  };

  // 시작일이 종료일보다 늦어지지 않게 다른 쪽을 맞춘다.
  const setFrom = (v: string) => v && onFilterChange({ ...filter, period: { from: v, to: v > to ? v : to } });
  const setTo = (v: string) => v && onFilterChange({ ...filter, period: { from: v < from ? v : from, to: v } });

  const ingestText =
    status.latestDate === null
      ? "적재된 로그가 없습니다"
      : `적재 현황: ${monthDay(status.latestDate)}까지 · 서버 ${num(status.serversOnLatest)}/${num(expectedServers)}`;

  const empty = data !== null && data.countries.length === 0;

  return (
    <div className="dashboard">
      <div className="bar">
        {PRESETS.map((p) => (
          <button
            key={p.key}
            type="button"
            className={`chip${filter.preset === p.key ? " on" : ""}`}
            onClick={() => choosePreset(p.key)}
          >
            {p.label}
          </button>
        ))}
        {filter.preset === "custom" && (
          <span className="range">
            <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} aria-label="시작일" />
            <span>~</span>
            <input type="date" value={to} onChange={(e) => setTo(e.target.value)} aria-label="종료일" />
          </span>
        )}
        <span className="sp" />
        <span className="muted">{ingestText}</span>
        <button type="button" className="chip" onClick={onOpenIngest}>
          로그 적재
        </button>
        <button type="button" className="chip" onClick={onOpenReport}>
          보고서 내보내기
        </button>
      </div>

      <div className="bar">
        <label className="toggle">
          <input
            type="checkbox"
            checked={exclude404}
            onChange={(e) => onFilterChange({ ...filter, exclude404: e.target.checked })}
          />
          <i aria-hidden="true" />
          404를 낸 IP 제외
        </label>
        <span className="muted">켜면 아래 모든 숫자·지도·보고서에서 빠집니다</span>
      </div>

      {error && <p className="error">데이터를 불러오지 못했습니다: {error}</p>}
      {data === null ? (
        !error && <p className="muted">불러오는 중…</p>
      ) : (
        <>
          <KpiTiles kpis={data.kpis} exclude404={exclude404} />
          {data.kpis.unknownRequests > 0 && (
            <p className="note">국가를 판별하지 못한 요청 {num(data.kpis.unknownRequests)}건은 집계에서 제외했습니다.</p>
          )}

          <div className="row map-row">
            <section className="panel">
              <h4>국가별 해외 요청 분포</h4>
              {empty ? <div className="empty box">{EMPTY}</div> : <WorldMap rows={data.countries} height={300} />}
            </section>
            <section className="panel">
              <h4>해외 상위 10개국 (요청 수)</h4>
              {empty ? <div className="empty box">{EMPTY}</div> : <TopCountries rows={data.countries} top={10} height={300} />}
            </section>
          </div>

          <section className="panel">
            <h4>일별 해외 요청 추이</h4>
            <DailyTrend points={data.daily} series={["requests", "ips"]} height={220} />
          </section>

          <section className="panel">
            <h4>국가별 상세 (열 제목을 누르면 정렬, 국가를 누르면 해당 국가의 IP 목록)</h4>
            {empty ? <div className="empty">{EMPTY}</div> : <CountryTable rows={data.countries} onSelect={setSelected} />}
          </section>
        </>
      )}

      <Notices />

      {selected !== null && <IpDrawer filter={query} country={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}
