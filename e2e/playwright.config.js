// E2E smoke do yggdrasil (YG-133). Roda contra BASE_URL (default: local).
//   local:  BASE_URL=http://localhost:3030 npx playwright test
//   prod:   BASE_URL=https://yggdrasil-artelonga.fly.dev npx playwright test
const { defineConfig } = require('@playwright/test');
module.exports = defineConfig({
  testDir: './tests',
  timeout: 30_000,
  retries: 1,
  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:3030',
    viewport: { width: 1280, height: 800 },
  },
  reporter: [['list']],
});
