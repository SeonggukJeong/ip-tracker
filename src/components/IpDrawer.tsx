// 국가를 누르면 열리는 옆 패널. 그 국가의 IP 목록(50개씩) → IP를 누르면 그 IP의 개별 요청(100개씩).
import { useCallback, useEffect, useRef, useState } from "react";
import { type Filter, getCountryIps, getIpRequests } from "../api";
import { countryName } from "../lib/countryName";
import { dateTime, dateTimeFull, num } from "../lib/format";

const IP_PAGE = 50;
const REQ_PAGE = 100;

type Paged<T> = { rows: T[]; done: boolean; loading: boolean; error: string | null };

/** key가 바뀌면 처음부터 다시 읽고, more()로 다음 쪽을 이어 붙인다. 늦게 온 이전 응답은 버린다. */
function usePaged<T>(key: string | null, pageSize: number, fetchPage: (limit: number, offset: number) => Promise<T[]>) {
  const [state, setState] = useState<Paged<T>>({ rows: [], done: false, loading: false, error: null });
  const fetchRef = useRef(fetchPage);
  fetchRef.current = fetchPage;
  const ticket = useRef(0);

  const load = useCallback(
    (offset: number, reset: boolean) => {
      const t = ++ticket.current;
      setState((s) => ({ rows: reset ? [] : s.rows, done: reset ? false : s.done, loading: true, error: null }));
      fetchRef.current(pageSize, offset).then(
        (page) => {
          if (t !== ticket.current) return;
          setState((s) => ({ rows: [...s.rows, ...page], done: page.length < pageSize, loading: false, error: null }));
        },
        (e) => {
          if (t !== ticket.current) return;
          setState((s) => ({ ...s, loading: false, error: String(e) }));
        },
      );
    },
    [pageSize],
  );

  useEffect(() => {
    if (key === null) {
      ticket.current++;
      setState({ rows: [], done: false, loading: false, error: null });
    } else {
      load(0, true);
    }
  }, [key, load]);

  return { ...state, more: () => load(state.rows.length, false) };
}

function PageFooter({ p, empty }: { p: Paged<unknown> & { more: () => void }; empty: boolean }) {
  if (p.error) return <p className="error">불러오지 못했습니다: {p.error}</p>;
  if (p.loading) return <p className="muted">불러오는 중…</p>;
  if (empty) return <p className="empty">해당 기간에 데이터가 없습니다</p>;
  if (!p.done)
    return (
      <button type="button" className="btn more" onClick={p.more}>
        더 보기
      </button>
    );
  return null;
}

export function IpDrawer({ filter, country, onClose }: { filter: Filter; country: string; onClose: () => void }) {
  const [ip, setIp] = useState<string | null>(null);
  const { period, exclude404 } = filter;
  const rangeKey = `${period.from}|${period.to}`;

  useEffect(() => setIp(null), [country, rangeKey, exclude404]);

  const ips = usePaged(`${country}|${rangeKey}|${exclude404}`, IP_PAGE, (limit, offset) =>
    getCountryIps({ period, exclude404 }, country, limit, offset),
  );
  const reqs = usePaged(ip === null ? null : `${ip}|${rangeKey}`, REQ_PAGE, (limit, offset) =>
    getIpRequests(period, ip ?? "", limit, offset),
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (ip !== null) setIp(null);
      else onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ip, onClose]);

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer" onClick={(e) => e.stopPropagation()}>
        <header className="drawer-head">
          {ip === null ? (
            <h3>{countryName(country)} IP 목록</h3>
          ) : (
            <>
              <button type="button" className="btn" onClick={() => setIp(null)}>
                ← IP 목록
              </button>
              <h3>{ip} 개별 요청</h3>
            </>
          )}
          <span className="muted">
            {period.from} ~ {period.to}
          </span>
          <span className="sp" />
          <button type="button" className="btn" onClick={onClose}>
            닫기
          </button>
        </header>

        <div className="drawer-body">
          {ip === null ? (
            <>
              <table className="grid clickable">
                <thead>
                  <tr>
                    <th>IP</th>
                    <th>요청 수</th>
                    <th>404 수</th>
                    <th>처음 접속</th>
                    <th>마지막 접속</th>
                  </tr>
                </thead>
                <tbody>
                  {ips.rows.map((r) => (
                    <tr key={r.ip} onClick={() => setIp(r.ip)}>
                      <td className="mono">{r.ip}</td>
                      <td>{num(r.requests)}</td>
                      <td>{num(r.notFound)}</td>
                      <td>{dateTime(r.firstTs)}</td>
                      <td>{dateTime(r.lastTs)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <PageFooter p={ips} empty={ips.done && ips.rows.length === 0} />
            </>
          ) : (
            <>
              <table className="grid requests">
                <thead>
                  <tr>
                    <th>시간</th>
                    <th>메서드</th>
                    <th>URL</th>
                    <th>상태</th>
                    <th>서버</th>
                    <th>User-Agent</th>
                  </tr>
                </thead>
                <tbody>
                  {reqs.rows.map((r, i) => (
                    <tr key={i}>
                      <td className="nowrap">{dateTimeFull(r.ts)}</td>
                      <td>{r.method}</td>
                      <td className="wrap">{r.url}</td>
                      <td className={r.status === 404 ? "warn" : undefined}>{r.status}</td>
                      <td>{r.server}</td>
                      <td className="wrap muted">{r.userAgent ?? "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <PageFooter p={reqs} empty={reqs.done && reqs.rows.length === 0} />
            </>
          )}
        </div>
      </aside>
    </div>
  );
}
