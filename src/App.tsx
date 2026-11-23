// 앱 틀: 상단 탭과 조회 조건(기간·404 제외). 조건은 대시보드와 보고서(Task 10)가 함께 쓴다.
import { useEffect, useState } from "react";
import { getIngestStatus, getSettings, type IngestStatus, type Settings } from "./api";
import { presetPeriod, todayKst, type ViewFilter } from "./lib/period";
import { Dashboard } from "./screens/Dashboard";

type Tab = "dashboard" | "ingest" | "settings";

const TABS: { key: Tab; label: string }[] = [
  { key: "dashboard", label: "대시보드" },
  { key: "ingest", label: "로그 적재" },
  { key: "settings", label: "설정" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [filter, setFilter] = useState<ViewFilter>(() => ({
    preset: "7d",
    period: presetPeriod("7d", null, todayKst()),
    exclude404: false,
  }));
  const [status, setStatus] = useState<IngestStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // 적재가 끝나면 올려서 적재 현황과 대시보드를 다시 읽는다. 올리는 쪽(적재 화면)은 Task 9에서 붙인다.
  const [refreshKey] = useState(0);

  useEffect(() => {
    let live = true;
    Promise.all([getIngestStatus(), getSettings()]).then(
      ([st, se]) => {
        if (!live) return;
        setStatus(st);
        setSettings(se);
        setLoadError(null);
        // 기간 버튼은 최근 적재일을 기준으로 한다. 적재 후에는 기준일이 바뀔 수 있으므로 다시 계산한다.
        setFilter((f) => (f.preset === "custom" ? f : { ...f, period: presetPeriod(f.preset, st.latestDate, todayKst()) }));
      },
      (e) => live && setLoadError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [refreshKey]);

  return (
    <div className="app">
      <nav className="tabs">
        <span className="brand">접속 국가 현황</span>
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            className={`tab${tab === t.key ? " on" : ""}`}
            onClick={() => setTab(t.key)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      <main className="content">
        {tab === "dashboard" &&
          (loadError ? (
            <p className="error">적재 현황을 불러오지 못했습니다: {loadError}</p>
          ) : status === null || settings === null ? (
            <p className="muted">불러오는 중…</p>
          ) : (
            <Dashboard
              filter={filter}
              onFilterChange={setFilter}
              status={status}
              expectedServers={settings.expectedServers}
              refreshKey={refreshKey}
              onOpenIngest={() => setTab("ingest")}
            />
          ))}
        {tab === "ingest" && (
          <p className="placeholder">로그 적재 화면은 준비 중입니다.</p>
        )}
        {tab === "settings" && <p className="placeholder">설정 화면은 준비 중입니다.</p>}
      </main>
    </div>
  );
}
