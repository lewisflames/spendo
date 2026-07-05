import { z } from 'zod';
import { query } from '$app/server';
import {
  fetchPrices,
  listCommodities,
  getCommodityPageData as getCommodityPageDataFn,
  getPriceHistory,
  type PricesResult,
  type CommodityInfo,
  type CommodityPageData,
  type PriceHistoryEntry
} from '$lib/server/prices';

const noParamsSchema = z.object({}).strict();

export const refreshPrices = query(
  z.object({ overrides: z.any().optional() }).strict(),
  async ({ overrides }: { overrides?: Record<string, string> }): Promise<PricesResult> => {
    return fetchPrices(overrides);
  }
);

export const getCommodities = query(
  noParamsSchema,
  async (_data: Record<string, never>): Promise<CommodityInfo[]> => {
    return listCommodities();
  }
);

export const loadCommodityPageData = query(
  noParamsSchema,
  async (_data: Record<string, never>): Promise<CommodityPageData> => {
    return getCommodityPageDataFn();
  }
);

export const getPriceHistoryForCommodity = query(
  z.object({ commodityGuid: z.string().min(1) }).strict(),
  async ({ commodityGuid }: { commodityGuid: string }): Promise<PriceHistoryEntry[]> => {
    return getPriceHistory(commodityGuid);
  }
);
