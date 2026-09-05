/**
 * 버전을 세 곳에 한꺼번에 맞춘다.
 *
 *   package.json · src-tauri/tauri.conf.json · src-tauri/Cargo.toml
 *
 * 셋이 어긋나면 업데이터가 새 버전을 알아보지 못한다. 손으로 고치지 않는다.
 *
 *   node scripts/set-version.mjs 0.2.0
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const version = process.argv[2]

if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('사용법: node scripts/set-version.mjs 0.2.0')
  process.exit(1)
}

function editJson(rel, key) {
  const path = join(root, rel)
  const text = readFileSync(path, 'utf8')
  const next = text.replace(
    new RegExp(`("${key}"\\s*:\\s*")\\d+\\.\\d+\\.\\d+(")`),
    `$1${version}$2`,
  )
  writeFileSync(path, next)
  console.log(`  ${rel}`)
}

function editToml(rel) {
  const path = join(root, rel)
  const text = readFileSync(path, 'utf8')
  const next = text.replace(/^version = "\d+\.\d+\.\d+"$/m, `version = "${version}"`)
  writeFileSync(path, next)
  console.log(`  ${rel}`)
}

console.log(`버전을 ${version} 로 맞춥니다.`)
editJson('package.json', 'version')
editJson('src-tauri/tauri.conf.json', 'version')
editToml('src-tauri/Cargo.toml')
console.log('완료. src-tauri/Cargo.lock 도 함께 커밋하세요 (cargo check 한 번이면 갱신됩니다).')
