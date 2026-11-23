import { useEffect, useState } from "react";
import { getAppInfo } from "./api";

// 임시 화면: 명령 연결 확인용으로 버전만 보여준다. 실제 화면은 이후 작업에서 만든다.
export default function App() {
  const [text, setText] = useState("불러오는 중…");

  useEffect(() => {
    getAppInfo()
      .then((info) => setText(`IP Tracker ${info.version}`))
      .catch((e) => setText(`오류: ${String(e)}`));
  }, []);

  return <main style={{ padding: 24 }}>{text}</main>;
}
