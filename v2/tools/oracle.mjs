// Black-box oracle runner: renders each input with the TypeScript engine's
// public API and records HTML, the resolved tree JSON and the time taken.
// Usage: node oracle.mjs CARVE_JS_DIR IN.json OUT.json (OUT.jsonl writes one record per line)
// IN.json: [{"input": "...", "opts": {...}}]
import { readFileSync, writeFileSync, openSync, writeSync, closeSync } from 'node:fs'

const [jsDir, inPath, outPath] = process.argv.slice(2)
const m = await import(jsDir + '/v2/src/index.ts')
const cases = JSON.parse(readFileSync(inPath, 'utf8'))
const lines = outPath.endsWith('.jsonl')
const fd = lines ? openSync(outPath, 'w') : null
const out = []
for (const c of cases) {
  const opts = c.opts ?? {}
  const rec = {}
  const t0 = process.hrtime.bigint()
  try {
    rec.html = m.render(c.input, opts)
  } catch (e) {
    rec.htmlError = String(e && e.message)
  }
  const t1 = process.hrtime.bigint()
  try {
    const r = m.resolveTree(m.parseTree(c.input), opts)
    rec.tree = m.encodeTree(r.document)
  } catch (e) {
    rec.treeError = String(e && e.message)
  }
  rec.renderNs = Number(t1 - t0)
  if (lines) writeSync(fd, JSON.stringify(rec) + '\n')
  else out.push(rec)
}
if (lines) closeSync(fd)
else writeFileSync(outPath, JSON.stringify(out))
