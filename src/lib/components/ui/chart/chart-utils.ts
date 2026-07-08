import type { Tooltip } from 'layerchart';
import { getContext, setContext, type Component } from 'svelte';

export const THEMES = { light: '', dark: '.dark' } as const;

export type ChartConfig = {
  [k in string]: {
    label?: string;
    icon?: Component;
  } & (
    | { color?: string; theme?: never }
    | { color?: never; theme: Record<keyof typeof THEMES, string> }
  );
};

export type TooltipPayload = Tooltip.TooltipSeries;

// Helper to extract item config from a payload.
export function getPayloadConfigFromPayload(
  config: ChartConfig,
  payload: TooltipPayload,
  key: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  data?: Record<string, any> | null
) {
  if (typeof payload !== 'object' || payload === null) return undefined;

  const payloadConfig =
    'config' in payload && typeof payload.config === 'object' && payload.config !== null
      ? payload.config
      : undefined;

  let configLabelKey: string = key;

  if (payload.key === key) {
    configLabelKey = payload.key;
  } else if (payload.label === key) {
    configLabelKey = payload.label;
  } else if (key in payload && typeof payload[key as keyof typeof payload] === 'string') {
    configLabelKey = payload[key as keyof typeof payload] as string;
  } else if (
    payloadConfig !== undefined &&
    key in payloadConfig &&
    typeof payloadConfig[key as keyof typeof payloadConfig] === 'string'
  ) {
    configLabelKey = payloadConfig[key as keyof typeof payloadConfig] as string;
  } else if (data != null && key in data && typeof data[key] === 'string') {
    configLabelKey = data[key] as string;
  }

  return configLabelKey in config ? config[configLabelKey] : config[key as keyof typeof config];
}

type ChartContextValue = {
  config: ChartConfig;
};

const chartContextKey = Symbol('chart-context');

export function setChartContext(value: ChartContextValue) {
  return setContext(chartContextKey, value);
}

export function useChart() {
  return getContext<ChartContextValue>(chartContextKey);
}

/** Month-aligned ticks spaced to fit the available pixel width. */
export function responsiveTimeTicks(scale: { domain(): unknown[]; range(): number[] }): Date[] {
  const [start, end] = scale.domain() as [Date, Date];
  if (+end <= +start) return [start];
  const pixelWidth = Math.abs(scale.range()[1] - scale.range()[0]);
  const labelWidth = 60;
  const maxTicks = Math.max(1, Math.floor(pixelWidth / labelWidth));
  const msPerMonth = 30 * 24 * 60 * 60 * 1000;
  const monthCount = Math.round((+end - +start) / msPerMonth);
  const step = Math.max(1, Math.ceil(monthCount / maxTicks));
  const ticks: Date[] = [];
  const d = new Date(start);
  d.setDate(1);
  d.setHours(0, 0, 0, 0);
  while (d <= end) {
    ticks.push(new Date(d));
    d.setMonth(d.getMonth() + step);
  }
  if (ticks.length === 0) ticks.push(new Date(start));
  return ticks;
}
