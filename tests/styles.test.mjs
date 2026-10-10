import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

const directory = new URL("../src/styles/", import.meta.url);
const tokens = readFileSync(new URL("base.css", directory), "utf8");
const definitions = new Map(
  [...tokens.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((match) => [match[1], match[2]]),
);

test("layout spacing follows the shared four-pixel scale", () => {
  for (let step = 2; step <= 4; step += 1) {
    assert.equal(definitions.get(`--space-${step}`), `${step * 4}px`);
  }
});

test("component token references resolve, while one-off pixels are allowed", () => {
  for (const name of readdirSync(directory).filter((name) => name.endsWith(".css"))) {
    const css = readFileSync(new URL(name, directory), "utf8");
    for (const match of css.matchAll(/var\((--[\w-]+)/g)) {
      assert.ok(definitions.has(match[1]) || match[1] === "--range-progress", `${name}: undefined ${match[1]}`);
    }
  }
});

test("development controls have spacing before and after the test button", () => {
  const css = readFileSync(new URL("settings.css", directory), "utf8");
  assert.match(css, /#local-test\s*>\s*\*\s*\+\s*\*\s*\{\s*margin-top:\s*var\(--space-3\);/);
  assert.match(css, /#test-input\s*\{[^}]*font:\s*inherit;/);
});

test("tokens load before component styles", () => {
  const entry = readFileSync(new URL("../src/main.ts", import.meta.url), "utf8");
  assert.ok(entry.indexOf('./styles/base.css') < entry.indexOf('./styles/controls.css'));
  assert.ok(entry.indexOf('./styles/controls.css') < entry.indexOf('./styles/settings.css'));
});