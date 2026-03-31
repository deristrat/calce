import { test, expect, type Page } from '@playwright/test'

const EMAIL = 'admin@njorda.se'
const PASSWORD = 'protectme'

async function login(page: Page) {
  await page.goto('/login')
  await page.fill('input[type="email"]', EMAIL)
  await page.fill('input[type="password"]', PASSWORD)
  await page.click('button[type="submit"]')
  await page.waitForURL('**/dashboard')
  await page.waitForSelector('.ds-stat', { timeout: 10_000 })
}

test('edit user name and verify it persists', async ({ page }) => {
  await login(page)

  // Navigate to Users page
  await page.locator('.ds-sidebar').getByRole('link', { name: 'Users' }).click()
  await expect(page).toHaveURL(/\/users/)
  await expect(page.locator('.ds-table')).toBeVisible()

  // Click the first user row
  await page.locator('.ds-table tbody tr').first().click()
  await expect(page.locator('.ds-page__title')).toBeVisible()

  // Read the current name
  const nameCell = page.locator('.ds-kv-grid__label:text("Name") + span')
  const originalName = await nameCell.textContent()

  // Enter edit mode
  await page.getByRole('button', { name: 'Edit' }).click()

  // Change the name
  const testName = `Test User ${Date.now()}`
  const nameInput = page.locator('input[placeholder="Name"]')
  await nameInput.clear()
  await nameInput.fill(testName)

  // Save
  await page.getByRole('button', { name: 'Save' }).click()

  // Wait for edit mode to close (Save button disappears)
  await expect(page.getByRole('button', { name: 'Save' })).toBeHidden()

  // Verify the name updated in the UI
  await expect(nameCell).toHaveText(testName)

  // Reload the page to confirm it persisted
  await page.reload()
  await expect(nameCell).toHaveText(testName)

  // Restore the original name
  await page.getByRole('button', { name: 'Edit' }).click()
  await nameInput.clear()
  await nameInput.fill(originalName || '')
  await page.getByRole('button', { name: 'Save' }).click()
  await expect(page.getByRole('button', { name: 'Save' })).toBeHidden()
})
