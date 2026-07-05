import { z } from 'zod';
import { query } from '$app/server';
import { getDashboardData, type DashboardData } from '$lib/server/dashboard-data';

const dashboardSchema = z.object({
  monthCount: z.number().int().positive().optional().nullable().default(null)
});

export const fetchDashboardData = query(
  dashboardSchema,
  async ({ monthCount }: z.infer<typeof dashboardSchema>): Promise<DashboardData> => {
    return getDashboardData(monthCount);
  }
);
