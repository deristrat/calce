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

// Inject a valid-looking token so we can test UI without hitting the login API
async function setFakeAuth(page: Page) {
  await page.goto('/login')
  await page.evaluate((email) => {
    const payload = btoa(JSON.stringify({ sub: 'admin', role: 'admin', email, exp: 9999999999 }))
    localStorage.setItem('access_token', `eyJhbGciOiJFZERTQSJ9.${payload}.sig`)
  }, EMAIL)
}

// ── Auth ──

test('redirects to login when not authenticated', async ({ page }) => {
  await page.goto('/dashboard')
  await expect(page).toHaveURL(/\/login/)
})

test('login with valid credentials', async ({ page }) => {
  await login(page)
  await expect(page).toHaveURL(/\/dashboard/)
  await expect(page.locator('.ds-page__title')).toHaveText('Dashboard')
})

test('login shows error with bad credentials', async ({ page }) => {
  await page.goto('/login')
  await page.fill('input[type="email"]', 'bad@example.com')
  await page.fill('input[type="password"]', 'wrong')
  await page.click('button[type="submit"]')
  await expect(page.locator('.ds-login__error')).toBeVisible()
})

test('redirects to dashboard if already logged in', async ({ page }) => {
  await login(page)
  await page.goto('/login')
  await expect(page).toHaveURL(/\/dashboard/)
})

// ── Navigation (uses real login for data-dependent tests) ──

test('sidebar navigation works', async ({ page }) => {
  await login(page)

  await page.getByRole('link', { name: 'Organizations' }).click()
  await expect(page).toHaveURL(/\/organizations/)
  await expect(page.locator('.ds-page__title')).toHaveText('Organizations')

  await page.getByRole('link', { name: 'Users' }).click()
  await expect(page).toHaveURL(/\/users/)
  await expect(page.locator('.ds-page__title')).toHaveText('Users')

  await page.getByRole('link', { name: 'Instruments' }).click()
  await expect(page).toHaveURL(/\/instruments/)
  await expect(page.locator('.ds-page__title')).toHaveText('Instruments')

  await page.getByRole('link', { name: 'Dashboard' }).click()
  await expect(page).toHaveURL(/\/dashboard/)
})

test('breadcrumbs show current location', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Instruments' }).click()
  await expect(page.locator('.ds-breadcrumbs')).toContainText('Instruments')
})

// ── Dashboard ──

test('dashboard shows stat cards', async ({ page }) => {
  await login(page)
  await expect(page.locator('.ds-stat')).toHaveCount(6)
})

// ── Organizations ──

test('organizations page loads table', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Organizations' }).click()
  await expect(page.locator('.ds-table')).toBeVisible()
})

// ── Users ──

test('users page has search and table', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Users' }).click()
  await expect(page.locator('.ds-search')).toBeVisible()
  await expect(page.locator('.ds-table')).toBeVisible()
})

// ── Instruments ──

test('instruments page has search and table', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Instruments' }).click()
  await expect(page.locator('.ds-search')).toBeVisible()
  await expect(page.locator('.ds-table')).toBeVisible()
})

test('clicking instrument row navigates to detail', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Instruments' }).click()
  await page.waitForSelector('.ds-table tbody tr')
  await page.locator('.ds-table tbody tr').first().click()
  await expect(page).toHaveURL(/\/instruments\/.+/)
  await expect(page.locator('.ds-back-link')).toBeVisible()
})

// ── Design System ──

test('design system showcase page loads', async ({ page }) => {
  await login(page)
  await page.getByRole('link', { name: 'Design System' }).click()
  await expect(page).toHaveURL(/\/design/)
  await expect(page.locator('.ds-page__title')).toHaveText('Design System')
})

// ── UI tests that don't need real API (use fake auth to avoid rate limits) ──

test('unknown route shows 404 page', async ({ page }) => {
  await setFakeAuth(page)
  await page.goto('/nonexistent')
  await expect(page.locator('.ds-empty__title')).toHaveText('404')
  await expect(page.locator('.ds-empty__action a')).toBeVisible()
})

test('theme toggle switches between light and dark', async ({ page }) => {
  await setFakeAuth(page)
  await page.goto('/design')
  await page.waitForSelector('.ds-sidebar')
  const html = page.locator('html')
  await expect(html).toHaveAttribute('data-theme', 'light')

  await page.locator('.ds-sidebar__user-action').first().click()
  await expect(html).toHaveAttribute('data-theme', 'dark')

  await page.locator('.ds-sidebar__user-action').first().click()
  await expect(html).toHaveAttribute('data-theme', 'light')
})

test('logout redirects to login', async ({ page }) => {
  await setFakeAuth(page)
  await page.goto('/design')
  await page.waitForSelector('.ds-sidebar')
  await page.locator('.ds-sidebar__user-action').last().click()
  await expect(page).toHaveURL(/\/login/)
})
