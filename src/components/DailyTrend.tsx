// 일별 해외 요청 수·해외 고유 IP 수 선 그래프. 두 값의 크기 차이가 커서 y축을 좌(요청)·우(IP)로 나눈다.
import { useMemo } from "react";
import type { DayPoint } from "../api";
import { COLORS } from "../lib/echarts";
import { monthDay, num } from "../lib/format";
import { type ChartCommon, useEChart } from "./useEChart";

type SeriesKey = "requests" | "ips";

const SERIES: Record<SeriesKey, { name: string; color: string; axisName: string; pick: (p: DayPoint) => number }> = {
  requests: { name: "해외 요청 수", color: COLORS.primary, axisName: "요청 수", pick: (p) => p.foreignRequests },
  ips: { name: "해외 고유 IP 수", color: COLORS.secondary, axisName: "IP 수", pick: (p) => p.foreignIps },
};

export function DailyTrend({
  points,
  series,
  height,
  renderer = "canvas",
  animation = true,
}: { points: DayPoint[]; series: SeriesKey[] } & ChartCommon) {
  // 호출부가 매번 새 배열을 넘겨도 차트를 다시 그리지 않도록 문자열로 비교한다.
  const seriesKey = series.slice(0, 2).join(",");
  const option = useMemo(() => {
    const keys = (seriesKey ? seriesKey.split(",") : []) as SeriesKey[];
    const axisLabel = { color: COLORS.muted, formatter: (v: number) => num(v) };
    return {
      color: keys.map((k) => SERIES[k].color),
      // x축 끝 날짜 글자는 축 끝을 중심으로 찍혀 절반이 오른쪽으로 나간다. 오른쪽 y축이 없으면 그만큼 여백을 둔다.
      grid: { left: 8, right: keys.length > 1 ? 8 : 24, top: 28, bottom: keys.length > 1 ? 28 : 4, containLabel: true },
      legend: keys.length > 1 ? { bottom: 0, icon: "roundRect", itemWidth: 10, itemHeight: 10, textStyle: { color: COLORS.muted } } : undefined,
      tooltip: {
        trigger: "axis",
        valueFormatter: (v: number) => num(v),
      },
      xAxis: {
        type: "category",
        boundaryGap: false,
        data: points.map((p) => monthDay(p.date)),
        axisLine: { lineStyle: { color: COLORS.border } },
        axisTick: { show: false },
        axisLabel: { color: COLORS.muted },
      },
      yAxis: keys.map((k, i) => ({
        type: "value",
        name: SERIES[k].axisName,
        nameTextStyle: { color: COLORS.muted, align: i === 0 ? "left" : "right" },
        position: i === 0 ? "left" : "right",
        minInterval: 1,
        axisLabel,
        splitLine: { show: i === 0, lineStyle: { color: COLORS.grid } },
      })),
      series: keys.map((k, i) => ({
        type: "line",
        name: SERIES[k].name,
        yAxisIndex: i,
        data: points.map(SERIES[k].pick),
        showSymbol: points.length <= 31,
        symbolSize: 5,
        lineStyle: { width: 2 },
      })),
    };
  }, [points, seriesKey]);

  const ref = useEChart(option, renderer, animation);
  return <div ref={ref} style={{ width: "100%", height }} />;
}
