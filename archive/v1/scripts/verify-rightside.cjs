#!/usr/bin/env node
/*
 * 右侧栏(Altas｜关联｜媒体)真链验证:登录 → 拉门三图标各自落标签 → 关联/媒体面板
 * → 跨卡去重 → 收起常驻挂载 → 首页单标签。
 * 用法:node scripts/verify-rightside.cjs
 * 退出码:0 通过;1 有硬问题。
 */
const fs = require('fs')
const PW_DIR = '/root/.npm/_npx/e41f203b7505f1fb/node_modules/playwright'
const { chromium } = require(PW_DIR)
const base = 'http://127.0.0.1:5173'
const WORK_ID = '01a093ca-0b19-735f-a55f-dced09f970c8' // 王者之剑:媒体数据富,去重可验
const SHOTS = '/root/WikiAltas/.shots'
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
let failed = false
const step = (s) => console.log(`  · ${s}`)
const ok = (cond, msg) => {
  console.log(`  ${cond ? '✓' : '✗ FAIL'} ${msg}`)
  if (!cond) failed = true
}
// 与后端/前端同款标题比对键(用于跨卡去重断言)
function normTitle(t) {
  return String(t).toLowerCase().replace(/[\s/\\·•,，.。:：!！?？、''""''《》〈〉【】\[\]()（）\-—~～<>&|+={}"]+/g, '')
}
function clash(a, b) {
  const ka = normTitle(a), kb = normTitle(b)
  if (!ka || !kb) return false
  if (ka === kb) return true
  return Math.min(ka.length, kb.length) >= 10 && (ka.includes(kb) || kb.includes(ka))
}

async function main() {
  fs.mkdirSync(SHOTS, { recursive: true })
  const browser = await chromium.launch({ headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] })
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
  const consoleErrors = []
  const pageErrors = []
  page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()) })
  page.on('pageerror', (e) => pageErrors.push(String(e)))

  try {
    // 1) 登录
    step('登录')
    await page.goto(base + '/login', { waitUntil: 'domcontentloaded', timeout: 30000 })
    await sleep(1200)
    const pw = page.locator('input[placeholder="请输入访问密码"]')
    if (await pw.count()) {
      await pw.fill('1234')
      await page.getByRole('button', { name: '进入管理态' }).click()
      await sleep(1500)
    }
    ok(page.url() !== base + '/login', '已进入管理态')

    // 2) 作品页:拉门三图标
    step('作品页:拉门三图标(AI/关联/媒体)')
    await page.goto(`${base}/w/${WORK_ID}`, { waitUntil: 'domcontentloaded', timeout: 30000 })
    await sleep(3000)
    const side = page.locator('.right-side')
    ok((await side.count()) === 1, '右侧栏已挂载')
    ok((await side.first().evaluate((el) => el.classList.contains('is-open'))) === false, '默认收起')
    const railBtns = page.locator('.right-side-rail-btn')
    ok((await railBtns.count()) === 3, `拉门含 3 图标(实际 ${await railBtns.count()})`)
    const railW = await side.first().evaluate((el) => el.getBoundingClientRect().width)
    ok(Math.round(railW) === 44, `拉门宽 44px(实际 ${railW})`)
    await page.screenshot({ path: SHOTS + '/rs-1-rail.png' })

    // 3) 拉门「关联」→ 必须落关联标签(bug 回归:此前会落 Altas)
    step("拉门点「关联」→ 落关联标签(回归)")
    await page.locator('.right-side-rail-btn[aria-label="展开 关联"]').click()
    await sleep(2000)
    ok(side.first().evaluate((el) => el.classList.contains('is-open')), '已展开')
    let sel = await page.locator('.right-side-tab[aria-selected="true"]').innerText()
    ok(sel.includes('关联') && !sel.includes('Altas'), `选中=关联(实际「${sel.trim()}」)`)
    const relHeads = page.locator('.work-side-tabpane .work-side-section-head')
    ok((await relHeads.count()) === 2, '关联标签=作品关系+库外链两段')
    ok(await page.locator('.work-side-body .work-side-empty').first().isVisible(), '空态为新版文案')
    await page.screenshot({ path: SHOTS + '/rs-2-relations.png' })

    // 4) 收起 → 拉门「媒体」→ 必须落媒体标签(拉门只在收起态可见,收起再开)
    step("收起 → 拉门点「媒体」→ 落媒体标签(回归)")
    await page.locator('.right-side-collapse').click()
    await sleep(700)
    await page.locator('.right-side-rail-btn[aria-label="展开 媒体"]').click()
    await sleep(3500)
    sel = (await page.locator('.right-side-tab[aria-selected="true"]').innerText()).trim()
    ok(sel.includes('媒体'), `选中=媒体(实际「${sel}」)`)
    const cards = await page.locator('.work-side-tabpane.is-active .ga-suggest').count()
    ok(cards >= 1, `媒体标签有内容卡(实际 ${cards})`)
    await page.screenshot({ path: SHOTS + '/rs-3-media.png' })

    // 5) 跨卡去重:「媒体库发现」长条 与 「Emby 影像」海报 不得有同条目
    step('跨卡去重断言')
    const stripTitles = await page.locator('.work-side-tabpane.is-active .ga-suggest-row .ga-suggest-title').allInnerTexts()
    const posterTitles = await page.locator('.work-side-tabpane.is-active .ga-poster-title').allInnerTexts()
    const dup = []
    for (const a of stripTitles) for (const b of posterTitles) if (clash(a, b)) dup.push([a.trim(), b.trim()])
    ok(dup.length === 0, `发现条 × 海报 无重复条目(${stripTitles.length} 条 / ${posterTitles.length} 张)${dup.length ? ' → ' + JSON.stringify(dup[0]) : ''}`)
    // 海报网格是 2 列
    const posters = await page.locator('.work-side-tabpane.is-active .ga-poster').count()
    ok(posters > 0, `海报网格已渲染(${posters} 张)`)

    // 6) 切回 Altas(标签条切换,不走拉门)
    step('标签条切回 Altas')
    await page.locator('.right-side-tab', { hasText: 'Altas' }).click()
    await sleep(1200)
    const panel = page.locator('.right-side .ai-panel')
    ok((await panel.count()) === 1 && await panel.isVisible(), 'Altas 面板可见')
    // 窄栏渲染体检:不得有"裁不掉"的横向撑破
    const overflow = await panel.first().evaluate((el) => {
      const bad = []
      el.querySelectorAll('*').forEach((n) => {
        const cs = getComputedStyle(n)
        const contained = /hidden|auto|scroll/.test(cs.overflowX)
        if (!contained && n.scrollWidth - n.clientWidth > 2) {
          bad.push(`${n.tagName}.${String(n.className).slice(0, 60)} sw=${n.scrollWidth} cw=${n.clientWidth} :: ${String(n.textContent).slice(0, 40)}`)
        }
      })
      return bad
    })
    ok(overflow.length === 0, `AI 面板内部无横向撑破节点(实际 ${overflow.length})${overflow.length ? ' → ' + overflow[0] : ''}`)

    // 7) 切标签/收起后 Altas 仍挂载(常驻)
    step('常驻挂载体检')
    await page.locator('.right-side-tab', { hasText: '媒体' }).click()
    await sleep(800)
    ok((await page.locator('.right-side .ai-panel').count()) === 1, '切标签后 Altas 仍挂载(SSE 不断)')
    await page.locator('.right-side-collapse').click()
    await sleep(800)
    ok((await page.locator('.right-side').first().evaluate((el) => el.classList.contains('is-open'))) === false, '已收起回拉门')
    ok((await page.locator('.right-side .ai-panel').count()) === 1, '收起后 Altas 仍挂载(收起不断流)')
    await page.screenshot({ path: SHOTS + '/rs-4-collapsed.png' })

    // 8) 首页:单标签形态
    step('首页:单标签形态')
    await page.goto(base, { waitUntil: 'domcontentloaded', timeout: 30000 })
    await sleep(2000)
    await page.locator('.right-side-rail-btn.is-ai').first().click()
    await sleep(1200)
    ok((await page.locator('.right-side-tab').count()) === 0, '首页无标签条(仅 Altas 标签页)')
    ok(await page.locator('.right-side .ai-panel').isVisible(), '首页 Altas 面板可见')
    await page.screenshot({ path: SHOTS + '/rs-5-home-single.png' })

    // 9) 控制台体检
    const hardErrors = consoleErrors.filter((t) => !t.includes('Download the React DevTools'))
    ok(hardErrors.length === 0, `无控制台错误(实际 ${hardErrors.length})${hardErrors.length ? ': ' + hardErrors[0].slice(0, 200) : ''}`)
    ok(pageErrors.length === 0, `无 pageerror(实际 ${pageErrors.length})${pageErrors.length ? ': ' + pageErrors[0].slice(0, 200) : ''}`)
  } catch (e) {
    failed = true
    console.error('EXCEPTION', e)
  }
  console.log(failed ? 'RESULT: FAIL' : 'RESULT: PASS')
  await browser.close()
  process.exit(failed ? 1 : 0)
}
main()
