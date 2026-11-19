import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [jsPath, seedText, countText, maxBytesText, output] = process.argv.slice(2)
if (!jsPath || !seedText || !countText || !maxBytesText || !output) process.exit(2)
const root = resolve(jsPath)
const { generate, loadCorpus, random } = await import(pathToFileURL(`${root}/proptest/generate.ts`).href)
process.chdir(root)
const corpus = loadCorpus()
const rng = random(Number(seedText))
const cases = Array.from({ length: Number(countText) }, () => ({ input: Buffer.from(generate(rng, corpus, Number(maxBytesText))).toString(), opts: {} }))
writeFileSync(output, JSON.stringify(cases))
