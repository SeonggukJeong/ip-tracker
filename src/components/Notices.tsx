// 화면 맨 아래 안내사항(문구 고정).
const NOTICES = [
  "국가는 접속 IP 주소 기준으로 판별하며, VPN 등으로 실제 위치와 다를 수 있습니다.",
  '"404를 낸 IP 제외"를 켜면, 없어진 주소를 호출하는 구버전 앱 이용자도 함께 빠질 수 있습니다.',
  "통신사·공용 와이파이처럼 여러 사람이 한 IP를 쓰는 경우, 한 명의 404로 같은 IP의 다른 이용자도 함께 빠질 수 있습니다.",
];

export function Notices() {
  return (
    <section className="notices">
      <h4>안내사항</h4>
      <ul>
        {NOTICES.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ul>
    </section>
  );
}
