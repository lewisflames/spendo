import { query } from '$app/server';
import {
  fetchPrices,
  listCommodities,
  getGnucashPrices,
  getCommodityPageData as getCommodityPageDataFn,
  type PricesResult,
  type CommodityInfo,
  type GnucashPriceEntry,
  type CommodityPageData
} from '$lib/server/prices';

export const refreshPrices = query(
  'unchecked',
  async ({
    token,
    overrides
  }: {
    token: string;
    overrides?: Record<string, string>;
  }): Promise<PricesResult> => {
    return fetchPrices(token, overrides);
  }
);

export const getCommodities = query(
  'unchecked',
  async ({ token }: { token: string }): Promise<CommodityInfo[]> => {
    return listCommodities(token);
  }
);

export const getFilePrices = query(
  'unchecked',
  async ({ token }: { token: string }): Promise<GnucashPriceEntry[]> => {
    return getGnucashPrices(token);
  }
);

export const loadCommodityPageData = query(
  'unchecked',
  async ({ token }: { token: string }): Promise<CommodityPageData> => {
    return getCommodityPageDataFn(token);
  }
);
