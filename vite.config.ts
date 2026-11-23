import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 개발 서버 설정(create-tauri-app 템플릿 기준).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // 앱 안에서 로컬 파일로 읽으므로 묶음 크기 경고 기준을 올린다(ECharts + 세계지도 데이터 포함 약 1MB).
  build: { chunkSizeWarningLimit: 1500 },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
