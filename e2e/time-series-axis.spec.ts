import { expect, mockBackend, test, type BackendMocks } from './fixtures';
import { DEMO_BOOK, TODAY, transactionsFor } from './screenshots/demo-book';

// Nearly four years of monthly points: enough for the axis to pick quarterly
// ticks on a wide chart, whose "Mon Year" labels are wider than the default
// tick spacing.
const MOCKS: BackendMocks = { ...DEMO_BOOK, fetch_transactions: transactionsFor(8) };

const CHARTS = [
  { path: '/', title: 'Net worth over time' },
  { path: '/portfolio', title: 'Portfolio performance' }
];

for (const width of [1024, 1280, 1440, 1600, 1920]) {
  for (const chart of CHARTS) {
    test(`${chart.title} keeps its date labels apart at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.clock.setFixedTime(new Date(`${TODAY}T10:30:00`));
      await mockBackend(page, MOCKS);
      await page.goto(chart.path);

      const card = page.locator('[data-slot="card"]', { hasText: chart.title });
      // The x axis centres its labels; the y axis right-aligns them.
      const labels = card.locator('.lc-axis-tick-label[text-anchor="middle"]');
      await expect(labels.first()).toBeVisible();

      const boxes = await labels.evaluateAll((els) =>
        els.map((el) => {
          const r = el.getBoundingClientRect();
          return { left: r.left, right: r.right, text: el.textContent };
        })
      );
      expect(boxes.length).toBeGreaterThanOrEqual(2);
      boxes.sort((a, b) => a.left - b.left);
      for (let i = 1; i < boxes.length; i++) {
        const gap = boxes[i].left - boxes[i - 1].right;
        expect(gap, `${boxes[i - 1].text} → ${boxes[i].text}`).toBeGreaterThanOrEqual(4);
      }
    });
  }
}
