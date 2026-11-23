// 국가별 해외 요청 분포 지도. 한 가지 색의 농도로 요청 수를 나타낸다. 한국은 색 계산에서 뺀다.
import { useMemo } from "react";
import type { CountryRow } from "../api";
import { countryName } from "../lib/countryName";
import { COLORS, esc, MAP_CODES } from "../lib/echarts";
import { num } from "../lib/format";
import { type ChartCommon, useEChart } from "./useEChart";

type Item = { name: string; value?: number; ips?: number; tooltip?: { show: boolean }; emphasis?: { disabled: boolean } };

export function WorldMap({ rows, height, renderer = "canvas", animation = true }: { rows: CountryRow[] } & ChartCommon) {
  const option = useMemo(() => {
    const data: Item[] = rows
      .filter((r) => r.country !== "KR")
      .map((r) => ({ name: r.country, value: r.requests, ips: r.ips }));
    // 한국과 코드가 없는 지역(-99)은 툴팁을 띄우지 않는다.
    for (const name of ["KR", "-99"]) data.push({ name, tooltip: { show: false }, emphasis: { disabled: true } });
    // 지도에 없는 나라가 1위여도 그려지는 나라 중 1위가 가장 진한 색이 되게, 그려지는 나라만으로 최댓값을 구한다.
    const max = Math.max(1, ...data.filter((d) => MAP_CODES.has(d.name)).map((d) => d.value ?? 0));
    return {
      tooltip: {
        trigger: "item",
        formatter: (p: { name: string; data?: Item }) =>
          `${esc(countryName(p.name))}<br>요청 ${num(p.data?.value ?? 0)}건 · IP ${num(p.data?.ips ?? 0)}개`,
      },
      visualMap: {
        type: "continuous",
        min: 0,
        max,
        show: false,
        inRange: { color: [COLORS.mapLight, COLORS.primary] },
        outOfRange: { color: COLORS.mapEmpty },
      },
      series: [
        {
          type: "map",
          map: "world",
          nameProperty: "ISO_A2_EH",
          roam: false,
          selectedMode: false,
          left: 0,
          right: 0,
          top: 0,
          bottom: 0,
          itemStyle: { areaColor: COLORS.mapEmpty, borderColor: "#c3ccd7", borderWidth: 0.5 },
          emphasis: { label: { show: false }, itemStyle: { areaColor: COLORS.secondary } },
          label: { show: false },
          data,
        },
      ],
    };
  }, [rows]);

  const ref = useEChart(option, renderer, animation);
  return <div ref={ref} style={{ width: "100%", height }} />;
}
