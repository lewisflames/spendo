import { z } from 'zod';
import { query } from '$app/server';
import {
  getTransactions,
  getAllTransactionsFiltered,
  getAccounts,
  type AccountInfo
} from '$lib/server/transactions';

const transactionsSchema = z.object({
  page: z.number().int().positive(),
  pageSize: z.number().int().positive(),
  filters: z
    .object({
      search: z.string().optional(),
      dateFrom: z.string().optional(),
      dateTo: z.string().optional(),
      accountName: z.string().optional()
    })
    .optional()
});

const filtersSchema = z
  .object({
    search: z.string().optional(),
    dateFrom: z.string().optional(),
    dateTo: z.string().optional(),
    accountName: z.string().optional()
  })
  .optional();

export const fetchTransactions = query(transactionsSchema, async ({ page, pageSize, filters }) => {
  return getTransactions(page, pageSize, filters);
});

export const fetchAllTransactions = query(
  z.object({ filters: filtersSchema }).strict(),
  async ({ filters }: { filters?: z.infer<typeof filtersSchema> }) => {
    return getAllTransactionsFiltered(filters);
  }
);

export const fetchAccountNames = query(
  'unchecked',
  async (_data: Record<string, never>): Promise<AccountInfo[]> => {
    return getAccounts();
  }
);
