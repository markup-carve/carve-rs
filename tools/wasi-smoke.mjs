// Runs the wasm32-wasip1 build of carve the way carve-go and hugo-carve do:
// `carve call` with one JSON request on standard input.
// Usage: node tools/wasi-smoke.mjs path/to/carve.wasm
import { readFile, writeFile, mkdtemp } from "node:fs/promises";
import { openSync, closeSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WASI } from "node:wasi";
import { setFlagsFromString } from "node:v8";
import { runInNewContext } from "node:vm";

// Node 22 segfaults once a dozen finished instances wait for collection.
setFlagsFromString("--expose-gc");
const gc = runInNewContext("gc");

const wasm = await WebAssembly.compile(await readFile(process.argv[2]));

async function call(request, preopens = {}) {
  const dir = await mkdtemp(join(tmpdir(), "carve-wasi-"));
  await writeFile(join(dir, "in.json"), JSON.stringify(request));
  const stdin = openSync(join(dir, "in.json"), "r");
  const stdout = openSync(join(dir, "out.json"), "w");
  const wasi = new WASI({ version: "preview1", args: ["carve", "call"], stdin, stdout, preopens, returnOnExit: true });
  const instance = await WebAssembly.instantiate(wasm, wasi.getImportObject());
  const status = wasi.start(instance);
  closeSync(stdin);
  closeSync(stdout);
  gc();
  return { status, response: JSON.parse(readFileSync(join(dir, "out.json"), "utf8")) };
}

const checks = [
  [{ call: "render", source: "Hi *there*" }, 0, (r) => r.ok.html === "<p>Hi <strong>there</strong></p>"],
  [{ call: "parse", source: "x", positions: false }, 0, (r) => r.ok.tree.type === "document"],
  [{ call: "lint", source: "# A\n\n# A\n" }, 0, (r) => Array.isArray(r.ok.diagnostics)],
  [{ call: "render", source: "x", options: { nope: 1 } }, 2, (r) => r.error.category === "usage"],
  // carve-rs#4327, #4328, #4329: the writer, the HTML importer and the standard extensions.
  [{ call: "format", source: "Hi _there_\n" }, 0, (r) => r.ok.source === "Hi \\_there_\n"],
  [
    { call: "write", tree: { type: "document", astVersion: "2.0", children: [{ type: "thematic_break" }] } },
    0,
    (r) => r.ok.output === "***\n",
  ],
  [
    { call: "importDocument", source: "<p>Hi <strong>there</strong></p>", from: "html" },
    0,
    (r) => r.ok.source === "Hi *there*\n" && r.ok.tree.type === "document",
  ],
  [
    { call: "render", source: ':::: tabs\n::: tab "A"\nx\n:::\n::::\n', options: { extensions: ["carve.tabs"], renderers: { "carve.tabs": "standard" } } },
    0,
    (r) => r.ok.html.startsWith('<div class="tabs"'),
  ],
];
let failed = 0;
for (const [request, status, ok] of checks) {
  const got = await call(request);
  if (got.status !== status || !ok(got.response)) {
    failed++;
    console.error("FAIL", JSON.stringify(request), got.status, JSON.stringify(got.response));
  }
}
const tree = (await call({ call: "parse", source: "# T" })).response.ok.tree;
const round = await call({ call: "decodeTree", json: JSON.stringify(tree) });
if (round.status !== 0 || round.response.ok.tree.type !== "document") {
  failed++;
  console.error("FAIL decodeTree", JSON.stringify(round.response));
}
// carve-rs#4326: a contained resolver reads a target inside its mounted root.
const root = await mkdtemp(join(tmpdir(), "carve-root-"));
await writeFile(join(root, "part.crv"), "*inside*\n");
const included = await call(
  { call: "renderWithDiagnostics", source: "{{ part.crv }}", options: { includes: { root: "/carve-root", file: "main.crv" } } },
  { "/carve-root": root },
);
if (included.status !== 0 || included.response.ok?.html !== "<p><strong>inside</strong></p>") {
  failed++;
  console.error("FAIL include", JSON.stringify(included.response));
}
const outside = await call(
  { call: "render", source: "{{ ../x.crv }}", options: { includes: { root: "/carve-root" } } },
  { "/carve-root": root },
);
if (outside.status !== 0 || !outside.response.ok?.html.includes("x.crv")) {
  failed++;
  console.error("FAIL include outside", JSON.stringify(outside.response));
}
console.log(failed ? `wasi smoke: ${failed} failed` : "wasi smoke: ok");
process.exit(failed ? 1 : 0);
