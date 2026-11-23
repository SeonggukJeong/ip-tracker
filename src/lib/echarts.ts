// ECharts 공통 설정: 필요한 부분만 등록하고, 내장 세계지도를 "world"로 등록한다.
// 지도 데이터는 Natural Earth 110m admin-0 countries(속성은 ISO_A2_EH만 남김). 네트워크를 쓰지 않는다.
import * as echarts from "echarts/core";
import { BarChart, LineChart, MapChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent, VisualMapComponent } from "echarts/components";
import { CanvasRenderer, SVGRenderer } from "echarts/renderers";
import world from "../assets/world.geo.json";

echarts.use([
  BarChart,
  LineChart,
  MapChart,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
  CanvasRenderer,
  SVGRenderer,
]);

type Geo = { type: "FeatureCollection"; features: { properties: { ISO_A2_EH: string } }[] };

// 남극은 해외 접속과 관계없이 지도 아래쪽 공간만 차지하므로 뺀다.
const geo = world as unknown as Geo;
const features = geo.features.filter((f) => f.properties.ISO_A2_EH !== "AQ");
echarts.registerMap("world", { ...geo, features } as unknown as Parameters<typeof echarts.registerMap>[1]);

/** 지도에 실제로 그려지는 국가 코드. 110m 지도에는 작은 나라(SG, HK, MO, BH, MT 등)가 없다. */
export const MAP_CODES: ReadonlySet<string> = new Set(features.map((f) => f.properties.ISO_A2_EH));

export { echarts };

export const COLORS = {
  primary: "#1f4e8c",
  secondary: "#c9792b",
  gray: "#9aa7b6",
  border: "#dde3ea",
  text: "#1c2430",
  muted: "#5b6878",
  grid: "#e6ebf0",
  mapEmpty: "#eef3f8",
  mapLight: "#c6d6ea",
} as const;

export const FONT = '"Malgun Gothic", system-ui, sans-serif';

/** 툴팁 HTML에 넣는 문자열 이스케이프 */
export function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
