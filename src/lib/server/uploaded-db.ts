import Database from 'better-sqlite3';
import { writeFileSync, unlinkSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import crypto from 'node:crypto';
import { parseGnucashDb, type GnucashSummary } from './gnucash-parser';
import { clearPriceCache } from './prices';

interface DbEntry {
  path: string;
  createdAt: number;
}

const dbs = new Map<string, DbEntry>();
const UPLOAD_DIR = join(tmpdir(), 'spendo-uploads');
const MAX_FILE_SIZE = 100 * 1024 * 1024;
const STALE_MS = 30 * 60 * 1000;
const ALLOWED_EXTENSIONS = ['.gnucash'];

if (!existsSync(UPLOAD_DIR)) {
  mkdirSync(UPLOAD_DIR, { recursive: true });
}

setInterval(() => {
  const now = Date.now();
  for (const [token, entry] of dbs) {
    if (now - entry.createdAt > STALE_MS) {
      clearPriceCache(token);
      try {
        unlinkSync(entry.path);
      } catch {
        /* ignore */
      }
      dbs.delete(token);
    }
  }
}, 60_000).unref();

export function getDbPath(token: string): string {
  const entry = dbs.get(token);
  if (!entry) throw new Error('Database session not found or expired');
  return entry.path;
}

export interface TableInfo {
  name: string;
  sql: string;
  rowCount: number;
}

export async function saveUploadedDb(
  file: File
): Promise<{ token: string; tables: TableInfo[]; gnucash: GnucashSummary }> {
  const ext = file.name.toLowerCase().slice(file.name.lastIndexOf('.'));
  if (!ALLOWED_EXTENSIONS.includes(ext)) {
    throw new Error('Unsupported file type. Only .gnucash files are accepted.');
  }
  if (file.size > MAX_FILE_SIZE) {
    throw new Error(`File too large. Maximum size is ${MAX_FILE_SIZE / 1024 / 1024} MB`);
  }

  const token = crypto.randomBytes(16).toString('hex');
  const tempPath = join(UPLOAD_DIR, `${token}${ext}`);

  const buffer = Buffer.from(await file.arrayBuffer());
  writeFileSync(tempPath, buffer);

  const db = new Database(tempPath, { readonly: true });
  try {
    const tables = db
      .prepare(
        `SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name`
      )
      .all() as { name: string; sql: string }[];

    const tablesWithCounts = tables.map((t) => {
      const row = db.prepare(`SELECT COUNT(*) as count FROM "${t.name}"`).get() as {
        count: number;
      };
      return { name: t.name, sql: t.sql ?? '', rowCount: row.count };
    });

    const gnucash = parseGnucashDb(db);

    dbs.set(token, { path: tempPath, createdAt: Date.now() });

    return { token, tables: tablesWithCounts, gnucash };
  } finally {
    db.close();
  }
}
