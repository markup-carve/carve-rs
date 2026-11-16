import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { gzipSync } from 'node:zlib'

const [jsPath, outputPath] = process.argv.slice(2)
if (!jsPath || !outputPath) process.exit(2)
const root = resolve(jsPath)
const data = resolve(dirname(fileURLToPath(import.meta.url)), '../tests/data')
const manifest = JSON.parse(readFileSync(join(data, 'corpus-seeds.json'), 'utf8'))
const differences = JSON.parse(readFileSync(join(data, 'corpus-differences.json'), 'utf8'))
const head = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim()
if (head !== manifest.oracleCommit) throw new Error(`oracle commit ${head} differs from ${manifest.oracleCommit}`)
const generator = await import(pathToFileURL(join(root, manifest.generator)).href)
const engine = await import(pathToFileURL(join(root, 'v2/src/index.ts')).href)
process.chdir(root)
const sourceCorpus = generator.loadCorpus()
const records = []
function record(id, input, opts) {
  const tree = engine.encodeTree(engine.resolveTree(engine.parseTree(input), opts).document)
  records.push(JSON.stringify({ id, input, opts, html: engine.render(input, opts), tree }))
}
for (const group of manifest.seeds) {
  const selected = new Set(group.indices)
  const rng = generator.random(group.seed)
  for (let index = 0; index < group.count; index++) {
    const input = Buffer.from(generator.generate(rng, sourceCorpus, group.maxBytes)).toString()
    if (selected.has(index)) record(`seed-${group.seed}-${index}`, input, {})
  }
  if (selected.size !== group.indices.length) throw new Error(`duplicate index for seed ${group.seed}`)
}
for (const difference of differences) record(difference.id, difference.input, difference.opts)
writeFileSync(resolve(outputPath), gzipSync(Buffer.from(records.join('\n') + '\n'), { level: 9 }))
console.log(`${records.length} documents`)
