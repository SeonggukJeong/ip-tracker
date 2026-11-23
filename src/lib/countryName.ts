// 국가 코드 → 한국어 국가명. 별도 데이터 없이 브라우저 내장 Intl을 쓴다.
const names = new Intl.DisplayNames(["ko"], { type: "region" });
const cache = new Map<string, string>();

/** "US" → "미국", 모르는 코드는 코드 그대로 */
export function countryName(iso2: string): string {
  let name = cache.get(iso2);
  if (name === undefined) {
    try {
      name = names.of(iso2.toUpperCase()) ?? iso2;
    } catch {
      name = iso2; // 형식이 맞지 않는 코드(A1 등)는 예외를 던진다
    }
    cache.set(iso2, name);
  }
  return name;
}
