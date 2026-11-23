// 차트 컴포넌트 공통: ECharts 인스턴스를 만들고, 컨테이너 크기가 바뀌면 맞추고, 내려갈 때 정리한다.
import { useEffect, useRef } from "react";
import type { ECharts, EChartsCoreOption } from "echarts/core";
import { echarts, FONT } from "../lib/echarts";

/** 보고서(Task 10)가 svg·애니메이션 없음·mm 크기로 같은 차트를 다시 쓴다. */
export type ChartCommon = { height: number | string; renderer?: "canvas" | "svg"; animation?: boolean };

export function useEChart(option: EChartsCoreOption, renderer: "canvas" | "svg", animation: boolean) {
  const elRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<ECharts | null>(null);

  useEffect(() => {
    const el = elRef.current;
    if (!el) return;
    const chart = echarts.init(el, undefined, { renderer });
    chartRef.current = chart;
    const ro = new ResizeObserver(() => chart.resize());
    ro.observe(el);
    return () => {
      ro.disconnect();
      chart.dispose();
      chartRef.current = null;
    };
  }, [renderer]);

  useEffect(() => {
    chartRef.current?.setOption(
      { textStyle: { fontFamily: FONT }, ...option, animation },
      { notMerge: true },
    );
  }, [option, animation, renderer]);

  return elRef;
}
