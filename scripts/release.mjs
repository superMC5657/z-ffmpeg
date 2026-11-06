#!/usr/bin/env node
/**
 * 自动化发版闭环脚本：
 *   1. 校验当前 main 分支与提交状态（支持可选的自动升版并提交推送）
 *   2. 自动检测并等待当前 commit 在 main 上的 CI 门禁 100% 全绿（支持实时轮询进度）
 *   3. 门禁全绿后，自动打 Tag 并推送到 GitHub，触发 release-tauri.yml 直通打包出包
 *
 * 用法:
 *   node scripts/release.mjs               # 针对当前版本，等待 CI 绿后打 Tag 推送
 *   node scripts/release.mjs --dry-run     # 预检 CI 状态，不实际打 Tag 或推送
 *   node scripts/release.mjs --patch       # 递增 patch 版本，提交推 main，等 CI 绿后打 Tag
 *   node scripts/release.mjs --minor       # 递增 minor 版本，提交推 main，等 CI 绿后打 Tag
 *   node scripts/release.mjs <x.y.z>       # 指定版本号，提交推 main，等 CI 绿后打 Tag
 */

import { execSync, spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')

function run(cmd, options = {}) {
  return execSync(cmd, { cwd: ROOT, encoding: 'utf8', ...options }).trim()
}

function runInherit(cmd, args) {
  const res = spawnSync(cmd, args, { cwd: ROOT, stdio: 'inherit' })
  if (res.status !== 0) {
    process.exit(res.status ?? 1)
  }
}

function getRepo() {
  try {
    const remoteUrl = run('git remote get-url origin')
    const match = remoteUrl.match(/github\.com[:/]([^/]+\/[^/.]+)/)
    if (match) return match[1].replace(/\.git$/, '')
  } catch {}
  return null
}

function getPkgVersion() {
  const pkg = JSON.parse(readFileSync(resolve(ROOT, 'package.json'), 'utf8'))
  return pkg.version
}

function checkGhCli() {
  try {
    run('gh --version')
  } catch {
    console.error('❌ 未检测到 GitHub CLI (gh)。请先安装 gh 命令行工具: https://cli.github.com/')
    process.exit(1)
  }
  try {
    run('gh auth status')
  } catch {
    console.error('❌ GitHub CLI 尚未登录。请先执行 `gh auth login` 登录 GitHub。')
    process.exit(1)
  }
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms))
}

function parseCliArgs() {
  const raw = process.argv.slice(2)
  const isDryRun = raw.includes('--dry-run')
  const isHelp = raw.includes('--help') || raw.includes('-h')
  const bumpArgs = raw.filter((a) => a !== '--dry-run' && a !== '--help' && a !== '-h')
  const hasBumpArg = bumpArgs.some(
    (a) => a === '--patch' || a === '--minor' || a === '--major' || /^\d+\.\d+\.\d+/.test(a),
  )
  return { isDryRun, isHelp, hasBumpArg, bumpArgs }
}

async function main() {
  const { isDryRun, isHelp, hasBumpArg, bumpArgs } = parseCliArgs()

  if (isHelp) {
    console.log(`
用法:
  node scripts/release.mjs [选项]

选项:
  --patch               递增 patch 版本号 (0.1.0 -> 0.1.1)
  --minor               递增 minor 版本号 (0.1.0 -> 0.2.0)
  --major               递增 major 版本号 (0.1.0 -> 1.0.0)
  <x.y.z>               直接指定版本号 (如 1.2.3)
  --dry-run             预检当前 Commit 的 CI 状态，不执行打 Tag 或推送
  -h, --help            显示帮助信息
`)
    process.exit(0)
  }

  checkGhCli()

  const repo = getRepo()
  if (!repo) {
    console.error('❌ 无法从 git remote 识别 GitHub 仓库 (origin)。')
    process.exit(1)
  }

  // 1. 检查分支
  const currentBranch = run('git rev-parse --abbrev-ref HEAD')
  if (currentBranch !== 'main' && currentBranch !== 'master') {
    console.error(`❌ 当前处于分支 [${currentBranch}]，发版必须在 main 分支进行！`)
    process.exit(1)
  }

  // 2. 如果指定了自动 bump
  if (hasBumpArg) {
    const status = run('git status --porcelain')
    if (status) {
      console.error('❌ 当前工作区有未提交的改动，请先提交或暂存后再执行发版！\n' + status)
      process.exit(1)
    }

    console.log('📦 正在执行版本号递增...')
    runInherit('node', ['scripts/bump-version.mjs', ...bumpArgs])

    const nextVersion = getPkgVersion()
    console.log(`📝 正在提交版本递增 commit: chore(release): bump to v${nextVersion}`)
    runInherit('git', [
      'add',
      'package.json',
      'src-tauri/Cargo.toml',
      'src-tauri/Cargo.lock',
      'src-tauri/tauri.conf.json',
    ])
    runInherit('git', ['commit', '-m', `chore(release): bump to v${nextVersion}`])

    if (!isDryRun) {
      console.log('⬆️ 正在推送到远端 main 分支...')
      runInherit('git', ['push', 'origin', currentBranch])
    }
  } else {
    // 检查是否有尚未推送的 commit
    const unpushed = run('git log @{u}..HEAD --oneline || true')
    if (unpushed) {
      if (isDryRun) {
        console.log('ℹ️ [dry-run] 本地有尚未推送到远端的提交:\n' + unpushed)
      } else {
        console.log('⬆️ 检测到本地有尚未推送到远端的提交，正在推送到 main...')
        runInherit('git', ['push', 'origin', currentBranch])
      }
    }
  }

  // 3. 校验目标 Tag
  const version = getPkgVersion()
  const tag = `v${version}`

  const remoteTag = run(`git ls-remote --tags origin refs/tags/${tag} || true`)
  if (remoteTag) {
    console.error(`❌ Tag [${tag}] 在远端仓库已存在！`)
    console.error(`   如果需要发布新版本，请运行: node scripts/release.mjs --patch`)
    process.exit(1)
  }

  const headSha = run('git rev-parse HEAD')
  console.log(`\n==================================================`)
  console.log(`🎯 目标版本:    ${tag}${isDryRun ? '  [DRY-RUN 预检模式]' : ''}`)
  console.log(`📍 目标 Commit:  ${headSha.slice(0, 8)} (${repo})`)
  console.log(`==================================================\n`)

  // 4. 检索 CI 任务
  console.log(`🔍 正在检索 GitHub Actions CI 任务...`)
  let runId = null
  let runUrl = null
  const searchStart = Date.now()
  const searchTimeout = 90 * 1000

  while (!runId && Date.now() - searchStart < searchTimeout) {
    try {
      const jsonStr = run(
        `gh run list --repo ${repo} --commit ${headSha} --workflow ci.yml --json databaseId,status,conclusion,url`,
      )
      const list = JSON.parse(jsonStr)
      if (list && list.length > 0) {
        runId = list[0].databaseId
        runUrl = list[0].url
        break
      }
    } catch {}
    process.stdout.write('.')
    await sleep(3000)
  }
  console.log('')

  if (!runId) {
    console.error(`❌ 未在 GitHub 上找到 Commit (${headSha.slice(0, 8)}) 的 CI 任务！`)
    console.error(`   请确认最新 commit 是否已推送至远端 main 分支。`)
    process.exit(1)
  }

  console.log(`🔗 对应 CI 任务: ${runUrl}`)

  // 5. 轮询 CI 状态
  const pollStart = Date.now()
  let finalConclusion = null

  while (true) {
    try {
      const viewJson = run(`gh run view ${runId} --repo ${repo} --json status,conclusion,jobs`)
      const info = JSON.parse(viewJson)

      if (info.status === 'completed') {
        finalConclusion = info.conclusion
        break
      }

      const elapsedSec = Math.floor((Date.now() - pollStart) / 1000)
      const jobsSummary = (info.jobs || [])
        .map((j) => `${j.name}: ${j.status === 'completed' ? (j.conclusion === 'success' ? '✔' : '✖') : '⏳'}`)
        .join(' | ')

      process.stdout.write(`\r⏳ CI 门禁运行中... [${elapsedSec}s] ${jobsSummary} `)
    } catch {}

    await sleep(5000)
  }
  console.log('\n')

  if (finalConclusion !== 'success') {
    console.error(`❌ CI 门禁未通过 (最终状态: ${finalConclusion})！`)
    console.error(`   查看失败日志: ${runUrl}`)
    console.error(`   ⛔ 已终止发版，未创建任何 Tag。`)
    process.exit(1)
  }

  console.log(`✨ CI 门禁已 100% 全绿通过 (success)！`)

  if (isDryRun) {
    console.log(`\n🎉 [DRY-RUN] 预检成功！当前 Commit 完全满足发版条件。未实际打 Tag 或推送。`)
    return
  }

  // 6. 门禁全绿，打 Tag 并推送到 GitHub 触发 Release
  console.log(`\n🏷️ 正在创建本地 Git Tag: ${tag}...`)
  try {
    run(`git tag -a ${tag} -m "Release ${tag}"`)
  } catch {
    run(`git tag -f -a ${tag} -m "Release ${tag}"`)
  }

  console.log(`🚀 正在推送 Tag [${tag}] 到 GitHub...`)
  runInherit('git', ['push', 'origin', tag])

  console.log(`\n==================================================`)
  console.log(`🎉 发版 Tag [${tag}] 推送成功！`)
  console.log(`📦 GitHub Actions 发版流水线 (release-tauri.yml) 已自动拉起直通构建打包。`)
  console.log(`🌐 实时查看发布进度: https://github.com/${repo}/actions`)
  console.log(`==================================================\n`)
}

main().catch((err) => {
  console.error('执行异常:', err)
  process.exit(1)
})
