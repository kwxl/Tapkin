import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
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

test("the variable UI font covers body, headings, and brand weights", () => {
  const entry = readFileSync(new URL("../src/main.ts", import.meta.url), "utf8");
  const settings = readFileSync(new URL("settings.css", directory), "utf8");
  const require = createRequire(import.meta.url);
  const weights = new Set([400]);
  for (const name of readdirSync(directory).filter((name) => name.endsWith(".css"))) {
    const css = readFileSync(new URL(name, directory), "utf8");
    for (const match of css.matchAll(/font-weight:\s*(\d+)\s*;/g)) {
      weights.add(Number(match[1]));
    }
  }
  const specifier = "@fontsource-variable/sn-pro";
  assert.ok(entry.includes(`import "${specifier}";`));
  assert.match(tokens, /"SN Pro Variable"/);
  assert.match(settings, /h1,\s*h2\s*\{\s*font-weight:\s*700;/);
  assert.match(settings, /\.brand\s*\{[^}]*font-weight:\s*700;/);
  const face = readFileSync(require.resolve(specifier), "utf8");
  assert.match(face, /font-family:\s*'SN Pro Variable'/);
  const range = face.match(/font-weight:\s*(\d+)\s+(\d+)\s*;/);
  assert.ok(range, "font must declare a variable weight range");
  const minimum = Number(range[1]);
  const maximum = Number(range[2]);
  for (const weight of weights) {
    assert.ok(weight >= minimum && weight <= maximum, `unsupported font weight ${weight}`);
    assert.ok(weight <= 700, `UI font weight ${weight} exceeds the maximum of 700`);
  }
});