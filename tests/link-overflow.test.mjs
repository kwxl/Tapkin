import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("../src/shared/link-overflow.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
});
const { visibleLinkCount } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);

test("all fitting links are visible without reserving More", () => {
  assert.equal(visibleLinkCount([20, 40], 68, 40, 8), 2);
  assert.equal(visibleLinkCount([], 0, 40, 8), 0);
});
test("overflow reserves More and a gap while preserving link order", () => {
  assert.equal(visibleLinkCount([20, 40, 50], 100, 30, 8), 1);
  assert.equal(visibleLinkCount([20, 40, 50], 106, 30, 8), 2);
  assert.equal(visibleLinkCount([100, 10], 80, 30, 8), 0);
  assert.equal(visibleLinkCount([20], 10, 30, 8), 0);
});
test("resizing restores links when space becomes available", () => {
  assert.equal(visibleLinkCount([30, 40, 50], 80, 30, 8), 1);
  assert.equal(visibleLinkCount([30, 40, 50], 136, 30, 8), 3);
});
test("fractional widths do not incorrectly hide fitting links", () => {
  assert.equal(visibleLinkCount([30.25, 40.25], 78.5, 30, 8), 2);
  assert.equal(visibleLinkCount([30.25, 40.25], 78.499999, 30, 8), 2);
  assert.equal(visibleLinkCount([30.25, 40.25], 78, 30, 8), 1);
});
test("author row reserves space and recomputes overflow after resize and fonts", () => {
  const css = readFileSync(new URL("../src/styles/settings.css", import.meta.url), "utf8");
  const author = readFileSync(new URL("../src/settings/author.ts", import.meta.url), "utf8");
  assert.match(css, /#skin-author\s*\{[^}]*display: flex;[^}]*height: 24px;/);
  assert.match(css, /#skin-author\.empty\s*\{\s*visibility: hidden;/);
  assert.match(author, /new ResizeObserver\(schedule\)/);
  assert.match(author, /document\.fonts\.ready\.then\(schedule\)/);
  assert.match(author, /anchors\.slice\(count\).*menu\.append/);
  assert.doesNotMatch(author, /links\.clientWidth/);
  assert.match(author, /const available = links\.getBoundingClientRect\(\)\.width/);
  assert.match(author, /visibleLinkCount\(widths, available, 0, gap\)/);
  assert.match(css, /#skin-author-name\s*\{\s*flex: 0 0 auto;/);
  assert.doesNotMatch(css, /max-width:\s*45%/);
  assert.match(author, /const reserved = Math\.min\(widths\[0\], moreWidth\)/);
  assert.match(author, /rowWidth - separation - reserved/);
});