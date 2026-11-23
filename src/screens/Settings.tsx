// 설정 화면: 기본·로그 형식(저장 버튼), IP 위치 DB 교체, 저장 공간(DB 위치 변경·오래된 해외 요청 삭제), 정보.
import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { getAppInfo, getSettings, purgeForeign, replaceGeoip, saveSettings, setDbPath, type AppInfo } from "../api";
import { num } from "../lib/format";
import { monthsBefore, todayKst } from "../lib/period";

type Form = { serviceName: string; expectedServers: string; logFormat: string; timeFormat: string; serverRegex: string };
type Section = "basic" | "log" | "geo" | "storage";
type Note = { kind: "ok" | "error"; text: string };

const MOVED_OK = "DB를 새 위치로 복사했습니다. 앱을 다시 시작하면 새 위치를 씁니다.";

/** "16" → 16. 1~999 정수가 아니면 null */
function parseServers(s: string): number | null {
  const t = s.trim();
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return n >= 1 && n <= 999 ? n : null;
}

/** "6" → 6. 1 이상 정수가 아니면 null */
function parseMonths(s: string): number | null {
  const t = s.trim();
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return Number.isSafeInteger(n) && n >= 1 ? n : null;
}

const mb = (n: number) =>
  `${(Number.isFinite(n) ? n / 1048576 : 0).toLocaleString("ko-KR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`;

export function Settings({ onChanged }: { onChanged: () => void }) {
  const [form, setForm] = useState<Form | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [notes, setNotes] = useState<Partial<Record<Section, Note>>>({});
  const [busy, setBusy] = useState(false);
  const [months, setMonths] = useState("6");
  const [moved, setMoved] = useState(false);
  const [confirm, setConfirm] = useState<{ date: string } | null>(null);

  useEffect(() => {
    let live = true;
    Promise.all([getSettings(), getAppInfo()]).then(
      ([s, a]) => {
        if (!live) return;
        setForm({
          serviceName: s.serviceName,
          expectedServers: String(s.expectedServers),
          logFormat: s.logFormat,
          timeFormat: s.timeFormat,
          serverRegex: s.serverRegex,
        });
        setInfo(a);
      },
      (e) => live && setLoadError(String(e)),
    );
    return () => {
      live = false;
    };
  }, []);

  if (loadError) return <p className="error">설정을 불러오지 못했습니다: {loadError}</p>;
  if (!form || !info) return <p className="muted">불러오는 중…</p>;

  const setNote = (s: Section, n: Note | undefined) => setNotes((p) => ({ ...p, [s]: n }));
  const edit = (k: keyof Form) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setForm({ ...form, [k]: e.target.value });
    setNote("basic", undefined);
    setNote("log", undefined);
  };

  async function saveForm(section: Section) {
    if (!form) return;
    const servers = parseServers(form.expectedServers);
    if (servers === null) {
      setNote(section, { kind: "error", text: "서버 대수는 1~999 사이의 정수로 입력해 주세요." });
      return;
    }
    setBusy(true);
    setNote(section, undefined);
    try {
      await saveSettings({
        serviceName: form.serviceName.trim(),
        expectedServers: servers,
        serverRegex: form.serverRegex.trim(),
        logFormat: form.logFormat.trim(),
        timeFormat: form.timeFormat.trim(),
      });
      setNote(section, { kind: "ok", text: "저장했습니다." });
      onChanged();
    } catch (e) {
      setNote(section, { kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function pickGeoip() {
    setNote("geo", undefined);
    setBusy(true);
    try {
      const path = await open({ multiple: false, filters: [{ name: "MMDB", extensions: ["mmdb"] }] });
      if (typeof path !== "string") return;
      const next = await replaceGeoip(path);
      setInfo(next);
      setNote("geo", { kind: "ok", text: `IP 위치 DB를 교체했습니다. 기준일 ${next.geoBuildDate}` });
      onChanged();
    } catch (e) {
      setNote("geo", { kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function moveDb() {
    setNote("storage", undefined);
    setBusy(true);
    try {
      const path = await save({ defaultPath: "ip-tracker.db", filters: [{ name: "SQLite DB", extensions: ["db"] }] });
      if (typeof path !== "string") return;
      await setDbPath(path);
      setMoved(true);
      setNote("storage", { kind: "ok", text: MOVED_OK });
    } catch (e) {
      setNote("storage", { kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  }

  function askPurge() {
    const n = parseMonths(months);
    if (n === null) {
      setNote("storage", { kind: "error", text: "개월 수는 1 이상의 정수로 입력해 주세요." });
      return;
    }
    setNote("storage", undefined);
    setConfirm({ date: monthsBefore(todayKst(), n) });
  }

  async function doPurge(date: string) {
    setConfirm(null);
    setBusy(true);
    try {
      const n = await purgeForeign(date);
      setNote("storage", { kind: "ok", text: `${num(n)}건을 지웠습니다.` });
      getAppInfo().then(setInfo, () => {});
      onChanged();
    } catch (e) {
      setNote("storage", { kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  }

  const NoteLine = ({ s }: { s: Section }) => {
    const n = notes[s];
    return n ? <p className={n.kind === "ok" ? "ok" : "error"}>{n.text}</p> : null;
  };

  return (
    <div className="settings">
      <section className="panel">
        <h4>기본</h4>
        <div className="set-form">
          <label>
            서비스 이름
            <input value={form.serviceName} onChange={edit("serviceName")} />
          </label>
          <p className="muted hint">보고서 제목에 사용합니다.</p>
          <label>
            서버 대수
            <input
              className="narrow"
              inputMode="numeric"
              value={form.expectedServers}
              onChange={edit("expectedServers")}
            />
          </label>
        </div>
        <div className="set-actions">
          <button type="button" className="btn primary" disabled={busy} onClick={() => saveForm("basic")}>
            저장
          </button>
        </div>
        <NoteLine s="basic" />
      </section>

      <section className="panel">
        <h4>로그 형식</h4>
        <div className="set-form">
          <label>
            아파치 LogFormat
            <input className="mono" value={form.logFormat} onChange={edit("logFormat")} />
          </label>
          <p className="muted hint">비우면 자동 인식합니다.</p>
          <label>
            시간 형식
            <input className="mono" value={form.timeFormat} onChange={edit("timeFormat")} />
          </label>
          <p className="muted hint">비우면 자동 인식합니다. 예시 %d/%b/%Y:%H:%M:%S %z</p>
          <label>
            서버 이름 추출 규칙
            <input className="mono" value={form.serverRegex} onChange={edit("serverRegex")} />
          </label>
          <p className="muted hint">정규식이며 첫 번째 괄호가 서버 이름입니다. 비우면 폴더 이름을 씁니다. 예시 ^(was\d+)_</p>
        </div>
        <div className="set-actions">
          <button type="button" className="btn primary" disabled={busy} onClick={() => saveForm("log")}>
            저장
          </button>
        </div>
        <NoteLine s="log" />
      </section>

      <section className="panel">
        <h4>IP 위치 DB</h4>
        <p>
          현재 DB 기준일 {info.geoBuildDate} <span className="muted">({info.geoIsBundled ? "내장 DB" : "교체한 DB"})</span>
        </p>
        <div className="set-actions">
          <button type="button" className="btn" disabled={busy} onClick={pickGeoip}>
            새 DB 파일 불러오기(.mmdb)
          </button>
        </div>
        <p className="muted hint">교체해도 이미 적재한 데이터의 국가는 바뀌지 않습니다.</p>
        <NoteLine s="geo" />
      </section>

      <section className="panel">
        <h4>저장 공간</h4>
        {info.dbWarning && <p className="warn">{info.dbWarning}</p>}
        <p>
          DB 위치 <span className="mono wrap">{info.dbPath}</span>
        </p>
        <p>DB 크기 {mb(info.dbBytes)}</p>
        <div className="set-actions">
          <button type="button" className="btn" disabled={busy || moved} onClick={moveDb}>
            DB 위치 변경
          </button>
        </div>
        <p className="muted hint">
          현재 DB를 새 위치로 복사하고, 다음 실행부터 새 위치를 씁니다. 기존 파일은 확인 후 직접 지워 주세요.
        </p>
        <h4 className="sub">오래된 해외 요청 상세 삭제</h4>
        <div className="set-actions">
          <input
            className="narrow"
            inputMode="numeric"
            aria-label="개월 수"
            value={months}
            onChange={(e) => {
              setMonths(e.target.value);
              setNote("storage", undefined);
            }}
          />
          <span>개월 이전</span>
          <button type="button" className="btn" disabled={busy} onClick={askPurge}>
            삭제
          </button>
        </div>
        <NoteLine s="storage" />
      </section>

      <section className="panel">
        <h4>정보</h4>
        <p>앱 버전 {info.version}</p>
        <p>IP 위치 정보: DB-IP (db-ip.com), CC BY 4.0</p>
      </section>

      {confirm && (
        <div className="drawer-backdrop">
          <div className="modal" role="dialog" aria-modal="true">
            <p>{confirm.date} 이전의 해외 요청 상세를 지웁니다. 국가별 통계는 그대로 남습니다. 계속할까요?</p>
            <div className="modal-actions">
              <button type="button" className="btn primary" onClick={() => doPurge(confirm.date)}>
                삭제
              </button>
              <button type="button" className="btn" onClick={() => setConfirm(null)}>
                취소
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
