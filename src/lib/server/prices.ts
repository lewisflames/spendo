import Database from 'better-sqlite3';
import crypto from 'node:crypto';
import { z } from 'zod';
import { getAppDbPath } from './app-db';

export interface CommodityInfo {
  guid: string;
  mnemonic: string;
  namespace: string;
  fullName: string | null;
}

export interface PriceEntry {
  mnemonic: string;
  price: number;
  priceDate: string;
  currency: string;
}

export interface PricesResult {
  prices: PriceEntry[];
  fetchedAt: string;
  errors: { mnemonic: string; error: string }[];
}

export interface GnucashPriceEntry {
  mnemonic: string;
  price: number;
  date: string;
  currency: string;
}

export interface PriceHistoryEntry {
  date: string;
  price: number;
  currency: string;
}

const NAMESPACE_YAHOO_SUFFIX: Record<string, string[]> = {
  EUREX: ['.DE'],
  XETRA: ['.DE'],
  EURONEXT: ['.AS', '.PA'],
  FUND: ['.AS', '.L', '.DE', '.PA'],
  MUTF: ['.AS', '.L', '.PA'],
  CRYPTO: ['-EUR'],
  NYSE: [''],
  NASDAQ: [''],
  AMEX: ['']
};

const DEFAULT_SUFFIXES = ['.DE', '.AS', '.L', '.PA', ''];

function openDb() {
  return new Database(getAppDbPath());
}

export function listCommodities(): CommodityInfo[] {
  const db = openDb();
  try {
    return db
      .prepare(
        `SELECT guid, mnemonic, namespace, fullname as full_name
				 FROM commodities
				 WHERE namespace != 'CURRENCY'
				 AND namespace != 'template'
				 ORDER BY namespace, mnemonic`
      )
      .all() as CommodityInfo[];
  } finally {
    db.close();
  }
}

type YahooFetchResult = { ok: true; price: number } | { ok: false; error: string };

export function getGnucashPrices(): GnucashPriceEntry[] {
  const db = openDb();
  try {
    const tableExists = db
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='prices'")
      .get() as { name: string } | undefined;
    if (!tableExists) return [];

    return db
      .prepare(
        `SELECT mnemonic, price, date, currency FROM (
					SELECT c.mnemonic,
						CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL) as price,
						p.date,
						cur.mnemonic as currency,
						ROW_NUMBER() OVER (PARTITION BY p.commodity_guid ORDER BY p.date DESC) as rn
					FROM prices p
					JOIN commodities c ON c.guid = p.commodity_guid
					JOIN commodities cur ON cur.guid = p.currency_guid
					WHERE c.namespace != 'CURRENCY'
				) WHERE rn = 1`
      )
      .all() as GnucashPriceEntry[];
  } finally {
    db.close();
  }
}

export function getLatestLivePrices(): PriceEntry[] {
  const db = openDb();
  try {
    const tableExists = db
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='prices'")
      .get() as { name: string } | undefined;
    if (!tableExists) return [];

    return db
      .prepare(
        `SELECT mnemonic, price, date as priceDate, currency FROM (
           SELECT c.mnemonic,
             CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL) as price,
             p.date,
             cur.mnemonic as currency,
             ROW_NUMBER() OVER (PARTITION BY c.mnemonic ORDER BY p.date DESC) as rn
           FROM prices p
           JOIN commodities c ON c.guid = p.commodity_guid
           JOIN commodities cur ON cur.guid = p.currency_guid
           WHERE p.guid LIKE 'LIVE-%'
             AND c.namespace != 'CURRENCY'
         ) WHERE rn = 1`
      )
      .all() as PriceEntry[];
  } finally {
    db.close();
  }
}

export function getLastRefreshTimestamp(): string | null {
  const db = openDb();
  try {
    const row = db
      .prepare("SELECT MAX(date) as max_date FROM prices WHERE guid LIKE 'LIVE-%'")
      .get() as { max_date: string | null } | undefined;
    return row?.max_date ?? null;
  } finally {
    db.close();
  }
}

export function getPriceHistory(commodityGuid: string): PriceHistoryEntry[] {
  const db = openDb();
  try {
    const rows = db
      .prepare(
        `SELECT p.date,
                CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL) as price,
                cur.mnemonic as currency,
                p.guid
         FROM prices p
         JOIN commodities cur ON cur.guid = p.currency_guid
         WHERE p.commodity_guid = ?
         ORDER BY p.date DESC`
      )
      .all(commodityGuid) as { date: string; price: number; currency: string; guid: string }[];

    return rows.map((r) => ({
      date: r.date,
      price: r.price,
      currency: r.currency
    }));
  } finally {
    db.close();
  }
}

export function getPreviousPrices(): { mnemonic: string; price: number; date: string }[] {
  const db = openDb();
  try {
    const tableExists = db
      .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='prices'")
      .get() as { name: string } | undefined;
    if (!tableExists) return [];

    return db
      .prepare(
        `SELECT mnemonic, price, date FROM (
          SELECT c.mnemonic,
            CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL) as price,
            p.date,
            ROW_NUMBER() OVER (PARTITION BY p.commodity_guid ORDER BY p.date DESC) as rn
          FROM prices p
          JOIN commodities c ON c.guid = p.commodity_guid
          WHERE c.namespace != 'CURRENCY'
        ) WHERE rn = 2`
      )
      .all() as { mnemonic: string; price: number; date: string }[];
  } finally {
    db.close();
  }
}

export interface CommodityPageData {
  commodities: CommodityInfo[];
  filePrices: GnucashPriceEntry[];
  livePrices: PriceEntry[];
  previousPrices: { mnemonic: string; price: number; date: string }[];
  lastRefreshed: string | null;
}

export function getCommodityPageData(): CommodityPageData {
  return {
    commodities: listCommodities(),
    filePrices: getGnucashPrices(),
    livePrices: getLatestLivePrices(),
    previousPrices: getPreviousPrices(),
    lastRefreshed: getLastRefreshTimestamp()
  };
}

function deriveYahooSymbols(mnemonic: string, namespace: string): string[] {
  const suffixes = NAMESPACE_YAHOO_SUFFIX[namespace];
  if (suffixes) return suffixes.map((s) => (s ? `${mnemonic}${s}` : mnemonic));
  return DEFAULT_SUFFIXES.map((s) => (s ? `${mnemonic}${s}` : mnemonic));
}

const yahooChartResponseSchema = z
  .object({
    chart: z.object({
      result: z
        .array(
          z.object({
            meta: z.object({
              regularMarketPrice: z.number()
            })
          })
        )
        .min(1)
    })
  })
  .nullable();

async function fetchSingleYahooSymbol(symbol: string): Promise<YahooFetchResult> {
  try {
    const url = `https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=1d&range=1d`;

    const response = await fetch(url, {
      headers: {
        'User-Agent':
          'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
        Accept: 'application/json'
      },
      signal: AbortSignal.timeout(10_000)
    });

    if (response.status === 429) {
      return { ok: false, error: 'Rate limited by Yahoo Finance' };
    }
    if (!response.ok) {
      return { ok: false, error: `HTTP ${response.status} from Yahoo` };
    }

    const data = await response.json();
    const parsed = yahooChartResponseSchema.safeParse(data);
    if (parsed.success) {
      const price = parsed.data!.chart.result[0].meta.regularMarketPrice;
      return { ok: true, price };
    }
    return { ok: false, error: 'Symbol not found on Yahoo' };
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Unknown error';
    return { ok: false, error: `Network error: ${message}` };
  }
}

async function fetchFromYahoo(
  symbols: string[]
): Promise<{ prices: Map<string, number>; errors: Map<string, string> }> {
  if (symbols.length === 0) return { prices: new Map(), errors: new Map() };

  const unique = [...new Set(symbols)];

  const results = await Promise.all(
    unique.map(async (sym) => {
      const result = await fetchSingleYahooSymbol(sym);
      return { sym, result } as const;
    })
  );

  const prices = new Map<string, number>();
  const errors = new Map<string, string>();
  for (const { sym, result } of results) {
    if (result.ok) {
      prices.set(sym, result.price);
    } else {
      errors.set(sym, result.error);
    }
  }

  return { prices, errors };
}

export async function fetchPrices(overrides?: Record<string, string>): Promise<PricesResult> {
  const commodities = listCommodities();

  const symbolMap = new Map<string, string>();
  const allSymbols: string[] = [];

  for (const c of commodities) {
    const override = overrides?.[c.guid];
    if (override) {
      if (!symbolMap.has(override)) {
        symbolMap.set(override, c.mnemonic);
        allSymbols.push(override);
      }
    } else {
      const candidates = deriveYahooSymbols(c.mnemonic, c.namespace);
      for (const sym of candidates) {
        if (!symbolMap.has(sym)) {
          symbolMap.set(sym, c.mnemonic);
          allSymbols.push(sym);
        }
      }
    }
  }

  const { prices: yahooPrices, errors: yahooErrors } = await fetchFromYahoo(allSymbols);
  const now = new Date().toISOString();

  const assigned = new Set<string>();
  const prices: PriceEntry[] = [];
  const errors: { mnemonic: string; error: string }[] = [];

  for (const [sym, mnemonic] of symbolMap) {
    if (assigned.has(mnemonic)) continue;
    const price = yahooPrices.get(sym);
    if (price != null) {
      prices.push({ mnemonic, price, priceDate: now, currency: 'EUR' });
      assigned.add(mnemonic);
    }
  }

  for (const c of commodities) {
    if (!assigned.has(c.mnemonic)) {
      const candidates = deriveYahooSymbols(c.mnemonic, c.namespace);
      const firstError = candidates.map((sym) => yahooErrors.get(sym)).filter(Boolean) as string[];
      const detail =
        firstError.length > 0 ? firstError[0] : `No price found (tried: ${candidates.join(', ')})`;
      errors.push({ mnemonic: c.mnemonic, error: detail });
    }
  }

  // Persist to DB
  if (prices.length > 0) {
    const db = openDb();
    try {
      const currencyRow = db
        .prepare("SELECT guid FROM commodities WHERE mnemonic = 'EUR' AND namespace = 'CURRENCY'")
        .get() as { guid: string } | undefined;

      const currencyGuid = currencyRow?.guid ?? null;

      if (currencyGuid) {
        const insert = db.prepare(
          `INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
           VALUES (?, ?, ?, ?, ?, ?)`
        );

        const insertMany = db.transaction(
          (entries: { guid: string; cGuid: string; price: number; date: string }[]) => {
            for (const e of entries) {
              insert.run(e.guid, e.cGuid, currencyGuid, Math.round(e.price * 10000), 10000, e.date);
            }
          }
        );

        const entries: { guid: string; cGuid: string; price: number; date: string }[] = [];
        for (const p of prices) {
          const commodity = commodities.find((c) => c.mnemonic === p.mnemonic);
          if (commodity) {
            entries.push({
              guid: `LIVE-${commodity.guid.slice(0, 8)}-${Date.now()}-${crypto.randomUUID().slice(0, 6)}`,
              cGuid: commodity.guid,
              price: p.price,
              date: p.priceDate
            });
          }
        }
        insertMany(entries);
      }
    } finally {
      db.close();
    }
  }

  return { prices, fetchedAt: now, errors };
}

export function getCachedPricesMap(): Map<string, { price: number; fetchedAt: string }> | null {
  const db = openDb();
  try {
    const rows = db
      .prepare(
        `SELECT mnemonic, price, date FROM (
           SELECT c.mnemonic,
             CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL) as price,
             p.date,
             ROW_NUMBER() OVER (PARTITION BY c.mnemonic ORDER BY p.date DESC) as rn
           FROM prices p
           JOIN commodities c ON c.guid = p.commodity_guid
           WHERE c.namespace != 'CURRENCY'
         ) WHERE rn = 1`
      )
      .all() as { mnemonic: string; price: number; date: string }[];

    if (rows.length === 0) return null;
    const map = new Map<string, { price: number; fetchedAt: string }>();
    for (const r of rows) map.set(r.mnemonic, { price: r.price, fetchedAt: r.date });
    return map;
  } finally {
    db.close();
  }
}

export function getUnifiedPricesMap(): Map<string, number> | null {
  const cached = getCachedPricesMap();
  if (cached && cached.size > 0) {
    const map = new Map<string, number>();
    for (const [mnemonic, entry] of cached) {
      map.set(mnemonic, entry.price);
    }
    return map;
  }

  const gnucashPrices = getGnucashPrices();
  if (gnucashPrices.length === 0) return null;

  const map = new Map<string, number>();
  for (const entry of gnucashPrices) {
    map.set(entry.mnemonic, entry.price);
  }
  return map;
}
