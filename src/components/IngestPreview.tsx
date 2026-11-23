// 미리보기: 인식한 형식, 앞 20줄의 해석 결과, 형식 직접 지정 칸.
import type { Preview } from "../api";
import { num } from "../lib/format";

export type FormatInput = { logFormat: string; timeFormat: string; save: boolean };

type Props = {
  filename: string | null;
  preview: Preview | null;
  loading: boolean;
  error: string | null;
  formatOpen: boolean;
  onOpenFormat: () => void;
  input: FormatInput;
  onInput: (v: FormatInput) => void;
  onRepreview: () => void;
  saveNote: string | null;
  disabled: boolean;
};

export function IngestPreview(p: Props) {
  const unrecognized = p.preview !== null && p.preview.logFormat === null;
  const showForm = p.formatOpen || unrecognized;

  return (
    <section className="panel">
      <div className="panel-head">
        <h4>미리보기{p.filename ? ` · ${p.filename}` : ""}</h4>
        <span className="sp" />
        {!showForm && p.preview && (
          <button type="button" className="btn" onClick={p.onOpenFormat} disabled={p.disabled}>
            형식 직접 지정
          </button>
        )}
      </div>

      {p.preview?.logFormat && (
        <p className="format-line">
          인식된 형식 <code className="mono">{p.preview.logFormat}</code>
        </p>
      )}
      {unrecognized && <p className="warn">로그 형식을 자동으로 인식하지 못했습니다. 아파치 LogFormat을 붙여 넣어 주세요.</p>}

      {showForm && (
        <div className="format-form">
          <label>
            <span>아파치 LogFormat</span>
            <input
              className="mono"
              value={p.input.logFormat}
              placeholder='%h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-Agent}i"'
              onChange={(e) => p.onInput({ ...p.input, logFormat: e.target.value })}
              disabled={p.disabled}
            />
          </label>
          <label>
            <span>시간 형식</span>
            <input
              className="mono"
              value={p.input.timeFormat}
              placeholder="비우면 자동 인식 (예: %d/%b/%Y:%H:%M:%S %z)"
              onChange={(e) => p.onInput({ ...p.input, timeFormat: e.target.value })}
              disabled={p.disabled}
            />
          </label>
          <div className="format-actions">
            <label className="check">
              <input
                type="checkbox"
                checked={p.input.save}
                onChange={(e) => p.onInput({ ...p.input, save: e.target.checked })}
                disabled={p.disabled}
              />
              이 형식을 저장
            </label>
            <button type="button" className="btn" onClick={p.onRepreview} disabled={p.disabled || p.loading}>
              다시 미리보기
            </button>
            {p.saveNote && <span className="muted">{p.saveNote}</span>}
          </div>
        </div>
      )}

      {p.error ? (
        <p className="error">미리보기를 만들지 못했습니다: {p.error}</p>
      ) : p.loading ? (
        <p className="muted">미리보기를 만드는 중…</p>
      ) : p.preview === null ? (
        <p className="empty">파일을 넣으면 앞 20줄을 읽어 보여줍니다</p>
      ) : (
        <>
          {p.preview.failed > 0 && (
            <p className="warn">
              앞 {num(p.preview.sampled)}줄 중 {num(p.preview.failed)}줄을 읽지 못했습니다.
            </p>
          )}
          {p.preview.rows.length > 0 && (
            <table className="grid preview">
              <thead>
                <tr>
                  <th>IP</th>
                  <th>시간</th>
                  <th>메서드</th>
                  <th>URL</th>
                  <th>상태코드</th>
                </tr>
              </thead>
              <tbody>
                {p.preview.rows.map((r, i) => (
                  <tr key={i}>
                    <td className="nowrap">{r.ip}</td>
                    <td className="nowrap">{r.time}</td>
                    <td>{r.method}</td>
                    <td className="wrap">{r.url}</td>
                    <td>{r.status}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </section>
  );
}
