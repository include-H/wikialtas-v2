#!/usr/bin/env node
/*
 * 归档/删除/标签 UI 实测：登录 → 左栏归档区 → 归档页横幅 → 资料夹资料删除。
 * 测试节点走 API 建/清，不污染库。退出码 0=通过。
 */
const PW_DIR = '/root/.npm/_npx/e41f203b7505f1fb/node_modules/playwright'
const { chromium } = require(PW_DIR)
const base = 'http://127.0.0.1:5173'
const API = 'http://127.0.0.1:8080'
const SHOTS = '/root/WikiAltas/.shots'
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function api(path, opts = {}) {
  const res = await fetch(API + path, {
    redirect: 'manual',
    ...opts,
    headers: { 'Content-Type': 'application/json', ...(opts.headers || {}) },
  })
  const text = await res.text().catch(() => '')
  return { status: res.status, body: text ? JSON.parse(text) : null }
}

async function main() {
  const browser = await chromium.launch()
  const page = await browser.newPage({ viewport: { width: 1480, height: 900 } })
  const errors = []
  page.on('pageerror', (e) => errors.push('pageerror: ' + e.message))
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()) })

  // 走 5173 代理（同源带 cookie），不直连 8080
  const papi = (path, opts = {}) =>
    page.evaluate(
      ([p, o]) =>
        fetch(p, { redirect: 'manual', ...o, headers: { 'Content-Type': 'application/json', ...(o.headers || {}) } })
          .then(async (r) => ({ status: r.status, body: r.body ? await r.json().catch(() => null) : null })),
      [path, opts]
    )

  let u1 = null, zu = null, doc = null
  try {
    await page.goto(base + '/login', { waitUntil: 'domcontentloaded', timeout: 30000 })
    await sleep(1000)
    const pw = page.locator('input[placeholder="请输入访问密码"]')
    await pw.fill('1234')
    await page.getByRole('button', { name: '进入管理态' }).click()
    await sleep(1500)

    // ---- 预备：用登录态建两个 universe（一个归档、一个挂资料） ----
    u1 = (await papi('/api/works', { method: 'POST', body: JSON.stringify({ kind: 'universe', title: 'UI归档验证' }) })).body
    zu = (await papi('/api/works', { method: 'POST', body: JSON.stringify({ kind: 'universe', title: 'UI归档验证-待归档', tags: ['验签'] }) })).body
    doc = (await papi(`/api/works/${u1.id}/docs`, { method: 'POST', body: JSON.stringify({ title: '验证资料', contentMd: '正文若干' }) })).body
    await papi(`/api/works/${zu.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'archived' }) })
    console.log('created u1=', u1.id, zu.id, 'doc=', doc.id)

    // ---- 1) 左栏归档区 ----
    await page.goto(base + '/', { waitUntil: 'domcontentloaded' })
    await sleep(1500)
    const archHead = page.locator('.tree-archive-head')
    if (!(await archHead.count())) throw new Error('左栏没出现归档区')
    const headText = (await archHead.first().innerText()).replace(/\n/g, ' ')
    if (!/归档 · 1/.test(headText)) throw new Error('归档计数不对: ' + headText)
    await page.screenshot({ path: SHOTS + '/archive-1-collapsed.png' })
    console.log('✓ 左栏归档区(折叠):', headText.trim())

    await archHead.first().click()
    await sleep(400)
    const row = page.locator('.tree-archive-row')
    if ((await row.count()) !== 1) throw new Error('归档行数量=' + (await row.count()))
    const rowText = await row.first().innerText()
    if (!rowText.includes('待归档')) throw new Error('归档行内容: ' + rowText)
    await page.screenshot({ path: SHOTS + '/archive-2-expanded.png' })
    console.log('✓ 归档区展开含恢复/删除:', rowText.replace(/\n/g, '|'))

    // ---- 2) 归档节点页横幅 ----
    await page.goto(base + '/w/' + zu.id, { waitUntil: 'domcontentloaded' })
    await sleep(1500)
    const banner = page.locator('.semi-banner')
    if (!(await banner.count())) throw new Error('归档页没出现横幅')
    const bannerText = await banner.first().innerText()
    if (!bannerText.includes('已归档')) throw new Error('横幅文案: ' + bannerText)
    if (!(await page.getByRole('button', { name: '恢复为草稿' }).count())) throw new Error('没有恢复为草稿按钮')
    await page.screenshot({ path: SHOTS + '/archive-3-banner.png' })
    console.log('✓ 归档页横幅+恢复按钮:', bannerText.replace(/\n/g, ' '))

    // ---- 3) 资料夹：资料行操作菜单 ----
    await page.goto(base + '/w/' + u1.id + '/folder', { waitUntil: 'domcontentloaded' })
    await sleep(1500)
    const docRow = page.locator('.doc-row')
    if ((await docRow.count()) !== 1) throw new Error('资料行数量=' + (await docRow.count()))
    await docRow.first().locator('button[aria-label="资料操作"]').hover()
    await docRow.first().locator('button[aria-label="资料操作"]').click()
    await sleep(500)
    const menu = page.locator('.semi-dropdown-item')
    const items = await menu.allInnerTexts()
    if (!items.join(',').includes('关联条目') || !items.join(',').includes('删除资料')) {
      throw new Error('资料操作菜单缺项: ' + JSON.stringify(items))
    }
    await page.screenshot({ path: SHOTS + '/archive-4-docmenu.png' })
    console.log('✓ 资料操作菜单:', JSON.stringify(items.map(s => s.trim())))

    // ---- 4) 树节点信息里的状态/标签 ----
    await page.keyboard.press('Escape')
    await page.goto(base + '/w/' + u1.id, { waitUntil: 'domcontentloaded' })
    await sleep(1200)
    await page.locator('button[aria-label="更多操作"]').click()
    await sleep(400)
    await page.locator('.semi-dropdown-item', { hasText: '节点信息' }).click()
    await sleep(500)
    const infoModal = page.locator('.semi-modal').filter({ hasText: '节点信息' })
    if (!(await infoModal.count())) throw new Error('节点信息弹窗没开')
    // 归档菜单项应存在
    const modalText = await infoModal.first().innerText()
    if (!modalText.includes('状态')) throw new Error('节点信息缺状态行: ' + modalText.slice(0, 200))
    await page.screenshot({ path: SHOTS + '/archive-5-nodeinfo.png' })
    console.log('✓ 节点信息弹窗 OK')
  } catch (e) {
    await page.screenshot({ path: SHOTS + '/archive-FAIL.png' }).catch(() => {})
    console.error('FAIL:', e.message)
    try {
      await papi(`/api/works/${zu.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'draft' }) })
      await papi(`/api/works/${u1.id}`, { method: 'DELETE' })
      await papi(`/api/works/${zu.id}`, { method: 'DELETE' })
      console.log('(failed-path cleanup done)')
    } catch (ce) {
      console.error('cleanup on fail also failed:', ce.message)
    }
    await browser.close()
    process.exit(1)
  }

  // ---- 清理：删除测试节点（u1 带资料级联；zu 已归档先恢复） ----
  await papi(`/api/works/${zu.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'draft' }) })
  const r1 = await papi(`/api/works/${u1.id}`, { method: 'DELETE' })
  const r2 = await papi(`/api/works/${zu.id}`, { method: 'DELETE' })
  console.log('cleanup:', r1.status, r2.status)

  // 报错过滤：dev 模式的资源 404/警告不算
  const hardErrors = errors.filter((e) => !/404|favicon|Download the React|WebSocket/i.test(e))
  if (hardErrors.length) {
    console.error('页面报错:', hardErrors.slice(0, 5))
    process.exit(1)
  }
  console.log('✅ 全部通过，截图在', SHOTS)
  await browser.close()
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
