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

test('fx rates page loads with table', async ({ page }) => {
  await login(page)
  await page.locator('.ds-sidebar').getByRole('link', { name: 'FX Rates' }).click()
  await expect(page).toHaveURL(/\/fx-rates/)
  await expect(page.locator('.ds-page__title')).toHaveText('FX Rates')
  await expect(page.locator('.ds-table')).toBeVisible()
  await expect(page.locator('.ds-search')).toHaveCount(3)
})

test('fx rates page shows cross-rate matrix', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  const matrix = page.locator('.ds-matrix')
  await expect(matrix).toBeVisible()
  // Matrix should have header cells for currencies
  const headers = matrix.locator('thead th')
  const count = await headers.count()
  // At least a few currencies + the empty corner cell
  expect(count).toBeGreaterThan(5)
})

test('clicking matrix cell navigates to detail', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  await page.waitForSelector('.ds-matrix')
  // Click a clickable cell (not identity, not empty)
  const clickable = page.locator('.ds-matrix__cell--clickable').first()
  await clickable.click()
  await expect(page).toHaveURL(/\/fx-rates\/[A-Z]+\/[A-Z]+/)
})

test('fx rates from filter works', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  await page.waitForSelector('.ds-table tbody tr')

  const rowsBefore = await page.locator('.ds-table tbody tr').count()

  // "From" filter is the first search input
  const fromInput = page.locator('.ds-search__input').nth(0)
  await fromInput.fill('USD')
  await page.waitForTimeout(500)

  await page.waitForSelector('.ds-table tbody tr')
  const rowsAfter = await page.locator('.ds-table tbody tr').count()

  // All visible rows should have USD as from_currency (2nd column)
  const firstRowFrom = await page.locator('.ds-table tbody tr').first().locator('td').nth(1).textContent()
  expect(firstRowFrom?.trim()).toBe('USD')
  expect(rowsAfter).toBeLessThanOrEqual(rowsBefore)
})

test('fx rates to filter works', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  await page.waitForSelector('.ds-table tbody tr')

  // "To" filter is the second search input
  const toInput = page.locator('.ds-search__input').nth(1)
  await toInput.fill('SEK')
  await page.waitForTimeout(500)

  await page.waitForSelector('.ds-table tbody tr')

  const firstRowTo = await page.locator('.ds-table tbody tr').first().locator('td').nth(2).textContent()
  expect(firstRowTo?.trim()).toBe('SEK')
})

test('clicking fx rate row navigates to detail page', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  await page.waitForSelector('.ds-table tbody tr')
  await page.locator('.ds-table tbody tr').first().click()
  await expect(page).toHaveURL(/\/fx-rates\/[A-Z]+\/[A-Z]+/)
  await expect(page.locator('.ds-back-link')).toBeVisible()
  await expect(page.locator('.ds-page__title')).toContainText('/')
})

test('fx rate detail page shows chart', async ({ page }) => {
  await login(page)
  await page.goto('/fx-rates')
  await page.waitForSelector('.ds-table tbody tr')
  await page.locator('.ds-table tbody tr').first().click()
  await expect(page).toHaveURL(/\/fx-rates\/[A-Z]+\/[A-Z]+/)

  const chartContainer = page.locator('.ds-chart-container')
  await expect(chartContainer).toBeVisible()

  // Wait for chart to render or error to show
  await page.waitForTimeout(3000)
  const hasCanvas = await chartContainer.locator('canvas').count()
  const hasNoData = await chartContainer.locator('p').count()
  const hasSpinner = await chartContainer.locator('.ds-spinner').count()

  console.log(`Chart state: canvas=${hasCanvas}, noData=${hasNoData}, spinner=${hasSpinner}`)
  if (hasCanvas === 0) {
    const text = await chartContainer.textContent()
    console.log('Chart container content:', text)
    // Check network for the API call
    const url = page.url()
    console.log('Current URL:', url)
  }

  expect(hasCanvas).toBeGreaterThan(0)
})
