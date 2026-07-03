import Database from 'better-sqlite3';
import { getDbPath } from './uploaded-db';

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

interface CachePrices {
  price: number;
  fetchedAt: string;
}

interface CacheEntry {
  prices: Map<string, CachePrices>;
  errors: { mnemonic: string; error: string }[];
  fetchedAt: string;
}

const priceCache = new Map<string, CacheEntry>();

export function clearPriceCache(token: string) {
  priceCache.delete(token);
}

export function listCommodities(token: string): CommodityInfo[] {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });
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

export function getGnucashPrices(token: string): GnucashPriceEntry[] {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });
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

export interface CommodityPageData {
  commodities: CommodityInfo[];
  filePrices: GnucashPriceEntry[];
}

export function getCommodityPageData(token: string): CommodityPageData {
  const commodities = listCommodities(token);
  const filePrices = getGnucashPrices(token);
  return { commodities, filePrices };
}

function deriveYahooSymbols(mnemonic: string, namespace: string): string[] {
  const suffixes = NAMESPACE_YAHOO_SUFFIX[namespace];
  if (suffixes) return suffixes.map((s) => (s ? `${mnemonic}${s}` : mnemonic));
  return DEFAULT_SUFFIXES.map((s) => (s ? `${mnemonic}${s}` : mnemonic));
}

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
    const price = data?.chart?.result?.[0]?.meta?.regularMarketPrice;
    if (typeof price === 'number') {
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

export async function fetchPrices(
  token: string,
  overrides?: Record<string, string>
): Promise<PricesResult> {
  const commodities = listCommodities(token);

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
      prices.push({ mnemonic, price, priceDate: now });
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

  const cachePrices = new Map<string, CachePrices>();
  for (const p of prices) {
    cachePrices.set(p.mnemonic, { price: p.price, fetchedAt: p.priceDate });
  }
  priceCache.set(token, { prices: cachePrices, errors, fetchedAt: now });

  return { prices, fetchedAt: now, errors };
}

export function getCachedPrice(token: string, mnemonic: string): number | null {
  const entry = priceCache.get(token);
  if (!entry) return null;
  const p = entry.prices.get(mnemonic);
  return p?.price ?? null;
}

export function getCachedPricesMap(
  token: string
): Map<string, { price: number; fetchedAt: string }> | null {
  const entry = priceCache.get(token);
  return entry?.prices ?? null;
}

export function getUnifiedPricesMap(token: string): Map<string, number> | null {
  const cached = getCachedPricesMap(token);
  if (cached && cached.size > 0) {
    const map = new Map<string, number>();
    for (const [mnemonic, entry] of cached) {
      map.set(mnemonic, entry.price);
    }
    return map;
  }

  const gnucashPrices = getGnucashPrices(token);
  if (gnucashPrices.length === 0) return null;

  const map = new Map<string, number>();
  for (const entry of gnucashPrices) {
    map.set(entry.mnemonic, entry.price);
  }
  return map;
}
