import { expect, mockBackend, NO_BOOK, test } from './fixtures';

test('shows the welcome screen without a book', async ({ page }) => {
  await mockBackend(page, NO_BOOK);
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Your money, kept on this machine.' })
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Get started' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recent books' })).toHaveCount(0);
});

test('lists recent books and marks missing files', async ({ page }) => {
  await mockBackend(page, { ...NO_BOOK, book_files_exist: [true, false] });
  await page.addInitScript(() => {
    const now = Math.floor(Date.now() / 1000);
    localStorage.setItem(
      'spendo-recent-books',
      JSON.stringify([
        { path: '/home/you/Finance/Household.spendo', openedAt: now - 3600 },
        { path: '/home/you/Old/Rental.spendo', openedAt: now - 86400 * 3 }
      ])
    );
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Household/ })).toBeEnabled();
  await expect(page.getByRole('button', { name: /^Rental/ })).toBeDisabled();
  await expect(page.getByText('File not found')).toBeVisible();

  await page.getByRole('button', { name: 'Remove Rental from recent books' }).click();
  await expect(page.getByRole('button', { name: /^Rental/ })).toHaveCount(0);
});
