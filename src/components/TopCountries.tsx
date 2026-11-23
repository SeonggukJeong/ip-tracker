// 해외 상위 국가 가로 막대. top개국 + 나머지를 합친 "기타 n개국"(회색).
import { useMemo } from "react";
import type { CountryRow } from "../api";
import { countryName } from "../lib/countryName";
import { COLORS, esc } from "../lib/echarts";
import { num } from "../lib/format";
import { type ChartCommon, useEChart } from "./useEChart";

type Bar = { name: string; value: number; ips: number; itemStyle?: { color: string } };

export function TopCountries({
  rows,
  top,
  height,
  renderer = "canvas",
  animation = true,
}: { rows: CountryRow[]; top: number } & ChartCommon) {
  const option = useMemo(() => {
    const sorted = [...rows].sort((a, b) => b.requests - a.requests);
    const bars: Bar[] = sorted.slice(0, top).map((r) => ({ name: countryName(r.country), value: r.requests, ips: r.ips }));
    const rest = sorted.slice(top);
    if (rest.length > 0) {
      bars.push({
        name: `기타 ${num(rest.length)}개국`,
        value: rest.reduce((s, r) => s + r.requests, 0),
        ips: rest.reduce((s, r) => s + r.ips, 0),
        itemStyle: { color: COLORS.gray },
      });
    }
    return {
      grid: { left: 0, right: 8, top: 0, bottom: 0, containLabel: true },
      tooltip: {
        trigger: "item",
        formatter: (p: { data: Bar }) => `${esc(p.data.name)}<br>요청 ${num(p.data.value)}건 · IP ${num(p.data.ips)}개`,
      },
      xAxis: { type: "value", show: false, max: (v: { max: number }) => Math.max(1, v.max) * 1.18 },
      yAxis: {
        type: "category",
        inverse: true,
        data: bars.map((b) => b.name),
        axisLine: { show: false },
        axisTick: { show: false },
        axisLabel: { color: COLORS.text },
      },
      series: [
        {
          type: "bar",
          data: bars,
          barMaxWidth: 14,
          itemStyle: { color: COLORS.primary, borderRadius: [0, 4, 4, 0] },
          label: { show: true, position: "right", color: COLORS.text, formatter: (p: { value: number }) => num(p.value) },
        },
      ],
    };
  }, [rows, top]);

  const ref = useEChart(option, renderer, animation);
  return <div ref={ref} style={{ width: "100%", height }} />;
}
