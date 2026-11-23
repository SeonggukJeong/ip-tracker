# IP Tracker

여러 대의 Apache 서버에서 쌓인 access 로그를 모아, 어떤 IP가 언제 어디서 접속했는지 보여 주는 Windows 데스크톱 도구다. 로그를 폴더째 읽어 로컬 DB에 적재하고, 대시보드·지도·보고서로 확인한다. 인터넷 없이 동작하도록 만들었고, 화면이 외부 주소를 부르는 일은 없다. 위치 정보도 설치 파일에 들어 있는 GeoIP 파일로만 찾는다.

Tauri 2(Rust)와 React, TypeScript, ECharts로 만들었다.

## 개발 실행

Node 22와 Rust stable이 필요하다.

```
npm install
npm run tauri dev
```

테스트는 `npm test`(화면 쪽)와 `cargo test --workspace`(Rust 쪽)로 돌린다.

## 가짜 로그 만들기

실제 로그가 없을 때는 `loggen`으로 Apache combined 형식의 가짜 로그를 서버별 폴더로 만든다.

```
cargo run --release -p loggen -- --out ./fake-logs --servers 16 --days 7 --per-day 20000
```

옵션은 다음과 같다.

- `--out` 출력 폴더(필수)
- `--servers` 서버 수, 기본 16
- `--days` 날짜 수, 기본 7
- `--end` 마지막 날짜(YYYY-MM-DD), 기본은 어제
- `--per-day` 서버 1대의 하루 정상 요청 수, 기본 20000
- `--foreign` 해외 IP 비율(0~1), 기본 0.02
- `--scanners` 404만 내는 스캐너 IP 수, 기본 30
- `--gzip` 마지막 날을 뺀 날짜를 `.gz`로 저장
- `--seed` 난수 시드, 기본 1

만든 폴더를 앱의 로그 적재 화면에서 고르면 된다.

## DB 크기

가짜 로그(서버 16대, 하루 2만 건씩, 하루 고유 IP 약 3만 2천 개)를 적재하면 하루치마다 DB가 약 21MB씩 커진다. 이 크기는 거의 전부 IP별 일별 요약이 차지하고, 해외 요청 상세는 일부에 그친다. 그래서 설정 화면에서 오래된 해외 요청 상세를 지워도 줄어드는 양은 많지 않다. 지워도 국가별 통계는 남는다. 실제 규모는 설정 화면에 표시되는 DB 크기로 확인한다.

## 릴리즈 만들기

설치 파일은 GitHub Actions가 Windows에서 만든다. 버전 태그를 푸시하면 된다.

```
git tag v0.1.0
git push origin v0.1.0
```

`v`로 시작하는 태그가 올라가면 `.github/workflows/release.yml`이 돌면서 `npm test`, `cargo test --workspace`, 빌드, 외부 주소 검사(`npm run check:offline`)를 차례로 통과시킨 뒤 NSIS 설치 파일을 만든다. 결과는 같은 태그의 GitHub 릴리즈에 초안으로 올라간다. 설치 파일을 내려받아 폐쇄망으로 반입하면 된다. 초안은 GitHub 릴리즈 화면에서 내용을 확인한 뒤 공개한다.

릴리즈 없이 Windows에서 테스트와 검사만 돌려 보고 싶으면 GitHub의 Actions 탭에서 `release` 워크플로를 수동 실행(Run workflow)한다. 이때는 설치 파일 만들기와 릴리즈 단계를 건너뛴다.

외부 주소 검사는 `dist/` 안의 html, js, css에서 `http://`, `https://` 주소를 찾아, 허용 목록에 없는 것이 있으면 실패한다. 허용 목록은 `scripts/check-offline.mjs`에 있고, 실제 요청이 아닌 XML 네임스페이스와 라이브러리의 문구만 정확한 주소로 올려 두었다.

## 폐쇄망 반입 후 첫 실행에서 확인할 것

처음 로그를 적재할 때 미리보기에서 IP와 시간이 제대로 읽히는지 본다. 맞지 않으면 설정 화면에서 LogFormat을 직접 지정한다.

시간대도 확인한다. 하루치 파일의 첫 줄과 끝 줄 시간이 KST 0시 무렵부터 24시 무렵까지여야 한다. rotatelogs가 UTC 기준으로 파일을 나누고 있으면 한 파일이 이틀에 걸치게 되고, 그러면 가장 최근 날짜는 9시간치만 보일 수 있다. 이때는 로그 쪽 설정이나 파일 나누는 기준을 먼저 바로잡는다.

## GeoIP 출처

IP의 국가 판별에는 DB-IP의 IP to Country Lite 데이터(db-ip.com)를 쓰며, 라이선스는 CC BY 4.0이다. 설정 화면과 보고서에 출처를 적어 두었다. 파일은 `src-tauri/resources/`에 있다.
