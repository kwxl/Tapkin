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

test("settings header uses the bundled app icon beside the brand", () => {
  const html = readFileSync(new URL("../src/index.html", import.meta.url), "utf8");
  const css = readFileSync(new URL("settings.css", directory), "utf8");
  assert.match(html, /class="brand-icon"\s+src="\/app-icon\.png"\s+alt=""/);
  const config = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");
  assert.match(config, /publicDir: "\.\.\/public"/);
  const icon = readFileSync(new URL("../public/app-icon.png", import.meta.url));
  assert.deepEqual(icon, readFileSync(new URL("../src-tauri/icons/128x128.png", import.meta.url)));
  assert.equal(icon.subarray(1, 4).toString(), "PNG");
  assert.match(css, /\.settings-view header\s*\{\s*display: flex;\s*align-items: center;/);
  assert.match(css, /\.brand-icon\s*\{[^}]*flex-shrink: 0;/);
  const header = html.match(/<header>([\s\S]*?)<\/header>/)[1];
  assert.ok(header.indexOf('class="brand-icon"') < header.indexOf('class="brand-copy"'));
});

test("radio controls match existing control colors and support disabled and checked states", () => {
  const css = readFileSync(new URL("controls.css", directory), "utf8");
  assert.match(css, /input\[type="radio"\]\s*\{[^}]*appearance: none;[^}]*border: var\(--border-width\) solid #dedbd5;[^}]*border-radius: 50%;[^}]*background: #fff;/);
  assert.match(css, /input\[type="radio"\]:checked\s*\{\s*background: #4c566b;/);
  assert.match(css, /input\[type="radio"\]:checked::after\s*\{[^}]*background: #fff;/);
  assert.match(css, /input\[type="radio"\]:disabled\s*\{[^}]*cursor: not-allowed;/);
  assert.match(css, /input:focus\s*\{\s*outline: 2px solid #5e839c;/);
  assert.match(css, /input\[type="radio"\]:focus\s*\{\s*outline: none;/);
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