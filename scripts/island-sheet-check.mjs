// node island-sheet-check.mjs <sanitize.js> <sheet.css>: exits 1 when
// Architect's `cleanSheet` refuses the sheet (scripts/island-sheet.sh).
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const [sanitize, sheet] = process.argv.slice(2);
const { cleanSheet } = await import(pathToFileURL(sanitize).href);
const css = readFileSync(sheet, 'utf8');
// The island's files are served under /_ext/island/<its folder>/.
const out = cleanSheet(css, '/_ext/island/studio.transport.0/');
if (out === null) {
  // cleanSheet refuses whole and says not why: name the likely causes.
  const hints = [...new Set(css.match(/(?:animation|transition)[a-z-]*:[^;}]*(?:var\(|infinite)[^;}]*/g) ?? [])];
  process.stderr.write(`island.css: refused by Architect's cleanSheet (${sanitize})\n${hints.map((h) => `  ${h}\n`).join('')}`);
  process.exit(1);
}
process.stdout.write(`island.css: kept by Architect's cleanSheet (${css.length} -> ${out.length} chars)\n`);
