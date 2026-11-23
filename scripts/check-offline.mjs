// dist/ 아래 .html/.js/.css에 외부 http(s) 주소가 남아 있으면 실패한다.
// 폐쇄망에서 쓰는 앱이라 런타임 외부 요청은 있으면 안 된다.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, extname } from "node:path";

const DIST = "dist";
const EXTS = new Set([".html", ".js", ".css"]);

// 정확한 주소 단위 허용 목록. 와일드카드 도메인 허용은 두지 않는다.
// 각 항목은 문자열 상수일 뿐 fetch/import/src 대상이 아님을 확인한 것이다.
const ALLOWED = new Set([
  // XML 네임스페이스 식별자(SVG 요소 생성용). 요청 대상이 아니다.
  "http://www.w3.org/2000/svg",
  "http://www.w3.org/1999/xhtml",
  "http://www.w3.org/1999/xlink",
  "http://www.w3.org/XML/1998/namespace",
  "http://www.w3.org/2000/xmlns/",
  // react-dom이 DOM 요소를 만들 때 고르는 MathML 네임스페이스 식별자. 요청 대상이 아니다.
  "http://www.w3.org/1998/Math/MathML",
  // react-dom 운영 빌드의 오류 메시지 문구. 오류 때 메시지 문자열에 이어 붙일 뿐
  // fetch/src로 쓰이지 않는다(Error 메시지에만 들어감).
  "https://reactjs.org/docs/error-decoder.html?invariant=",
  // GeoIP 데이터(DB-IP Lite) 출처 표기 문구. 화면에 글자로만 표시한다.
  "https://db-ip.com",
]);

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* walk(p);
    else if (EXTS.has(extname(p))) yield p;
  }
}

let files;
try {
  files = [...walk(DIST)];
} catch {
  console.error("dist/ 가 없다. 먼저 npm run build 를 실행한다.");
  process.exit(2);
}

const found = [];
for (const f of files) {
  const text = readFileSync(f, "utf8");
  for (const m of text.matchAll(/https?:\/\/[^\s"'`)<>\\]+/g)) {
    if (!ALLOWED.has(m[0])) found.push(`${f}: ${m[0]}`);
  }
}

if (found.length > 0) {
  console.error("외부 주소 발견:");
  for (const l of found) console.error("  " + l);
  process.exit(1);
}
console.log("외부 주소 없음");
