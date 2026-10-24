import { expect, mockBackend, NO_BOOK, test } from './fixtures';

test('shows the welcome screen without a book', async ({ page }) => {
  await mockBackend(page, NO_BOOK);
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Your money, kept on this machine.' })
  ).toBeVisible();
});
