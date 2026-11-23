// 국가별 상세 표. 열 제목을 누르면 정렬, 행을 누르면 onSelect(국가 코드).
import { useMemo, useState } from "react";
import type { CountryRow } from "../api";
import { countryName } from "../lib/countryName";
import { dateTime, num, pct } from "../lib/format";

type Key = "country" | "requests" | "share" | "ips" | "notFound" | "ips404" | "lastTs";

const COLUMNS: { key: Key; title: string; cell: (r: CountryRow) => string }[] = [
  { key: "country", title: "국가", cell: (r) => countryName(r.country) },
  { key: "requests", title: "요청 수", cell: (r) => num(r.requests) },
  { key: "share", title: "해외 중 비율", cell: (r) => pct(r.share) },
  { key: "ips", title: "고유 IP", cell: (r) => num(r.ips) },
  { key: "notFound", title: "404 요청 수", cell: (r) => num(r.notFound) },
  { key: "ips404", title: "404를 낸 고유 IP", cell: (r) => num(r.ips404) },
  { key: "lastTs", title: "최근 접속", cell: (r) => dateTime(r.lastTs) },
];

export function CountryTable({ rows, onSelect }: { rows: CountryRow[]; onSelect: (country: string) => void }) {
  const [sort, setSort] = useState<{ key: Key; desc: boolean }>({ key: "requests", desc: true });

  const sorted = useMemo(() => {
    const cmp =
      sort.key === "country"
        ? (a: CountryRow, b: CountryRow) => countryName(a.country).localeCompare(countryName(b.country), "ko")
        : (a: CountryRow, b: CountryRow) => (a[sort.key] as number) - (b[sort.key] as number);
    return [...rows].sort((a, b) => (sort.desc ? -cmp(a, b) : cmp(a, b)));
  }, [rows, sort]);

  // 국가명은 가나다순(오름차순)부터, 숫자는 큰 값(내림차순)부터 정렬한다.
  const toggle = (key: Key) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "country" }));

  return (
    <table className="grid clickable">
      <thead>
        <tr>
          {COLUMNS.map((c) => (
            <th key={c.key} onClick={() => toggle(c.key)} className="sortable">
              {c.title}
              <span className="sort-mark">{sort.key === c.key ? (sort.desc ? "▼" : "▲") : ""}</span>
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {sorted.map((r) => (
          <tr key={r.country} onClick={() => onSelect(r.country)}>
            {COLUMNS.map((c) => (
              <td key={c.key}>{c.cell(r)}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
