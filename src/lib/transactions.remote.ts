import { z } from 'zod';
import { query } from '$app/server';
import {
  getTransactions,
  getAllTransactionsFiltered,
  getAccounts,
  type Filters,
  type AccountInfo
} from '$lib/server/transactions';

const transactionsSchema = z.object({
  token: z.string().min(1),
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

export const fetchTransactions = query(
  transactionsSchema,
  async ({ token, page, pageSize, filters }) => {
    return getTransactions(token, page, pageSize, filters);
  }
);

export const fetchAllTransactions = query(
  'unchecked',
  async ({ token, filters }: { token: string; filters?: Filters }) => {
    return getAllTransactionsFiltered(token, filters);
  }
);

export const fetchAccountNames = query(
  'unchecked',
  async ({ token }: { token: string }): Promise<AccountInfo[]> => {
    return getAccounts(token);
  }
);
