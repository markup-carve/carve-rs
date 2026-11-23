// Writes tests/data/heading-bare-html-carve-js.json: the bare-html-tag ranges carve-js reports
// for every heading variant of tests/lint_heading_bare_html_sweep.rs (UTF-8 byte offsets, as
// carve-js reports them).
// Usage: node tools/heading_bare_html_golden.mjs CARVE_JS_DIR OUT.json
// Keep the variant lists in step with the Rust test, which checks the inputs match.
import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'

const [jsDir, outPath] = process.argv.slice(2)
const { lint } = await import(jsDir + '/src/index.ts')
const commit = execFileSync('git', ['-C', jsDir, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()

const quotes = ['', '> ', '> > ', '- ', '> - ', '::: note\n']
const befores = ['', 'a ', 'word ', '\\', '`code` ', '<bad ', '\\<x> ', '<!-- <x> --> ', '\\<!-- ', '`<!--` ', '<xé> ']
const tags = ['<x>', '</x>', '<x/>', '<x a="1">', '<b>', '<x:y>', '<u>', '<em>', '<a href="x">', '<x-1>', '<X>', '<tag data-x="v">']

const cases = []
for (let level = 1; level <= 6; level++) {
  for (const quote of quotes) {
    for (const before of befores) {
      for (const tag of tags) {
        const content = `${quote}${'#'.repeat(level)} ${before}${tag} c\n`
        const input = quote === '::: note\n' ? `${content}:::\n` : content
        const ranges = lint(input, {})
          .filter((d) => d.code === 'bare-html-tag')
          .map((d) => [d.range.startOffset, d.range.endOffset])
        cases.push([input, ranges])
      }
    }
  }
}
writeFileSync(outPath, JSON.stringify({ carve_js: commit, cases }) + '\n')
