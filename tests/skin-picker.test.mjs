import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const source = readFileSync(new URL("../src/settings/index.ts", import.meta.url), "utf8");
const html = readFileSync(new URL("../src/index.html", import.meta.url), "utf8");
const author = readFileSync(new URL("../src/settings/author.ts", import.meta.url), "utf8");

test("skin chooser uses repository IDs instead of an external folder picker", () => {
  assert.doesNotMatch(source, /@tauri-apps\/plugin-dialog/);
  assert.match(source, /invoke<SkinList>\("list_skins"\)/);
  assert.match(source, /invoke\("load_skin", \{ id \}\)/);
  assert.match(source, /skinDialog\.showModal\(\)/);
  assert.match(source, /radio\.disabled = skin\.error !== null/);
  assert.match(source, /skinCancel\.addEventListener\("click", \(\) => skinDialog\.close\(\)\)/);
  assert.match(html, /<dialog id="skin-picker" aria-labelledby="skin-picker-title">/);
  for (const id of ["choose-skin", "open-folder", "reload", "skin-choices", "skin-apply", "skin-cancel"]) {
    assert.ok(html.includes(`id="${id}"`));
  }
});

test("skin card hides filesystem paths and renders author links through a checked command", () => {
  assert.doesNotMatch(html, /id="skin-path"/);
  assert.doesNotMatch(source, /field\("skin-path"\)/);
  assert.match(html, /id="skin-author" class="empty" aria-hidden="true"/);
  assert.match(author, /row\.classList\.toggle\("empty", !author\)/);
  assert.match(author, /link\.textContent = label/);
  assert.match(source, /invoke\("open_author_link", \{ label \}\)/);
});

test("skin chooser shows the validated idle image next to each valid skin", () => {
  assert.match(source, /if \(skin\.idle_image\)/);
  assert.match(source, /idle\.src = skin\.idle_image/);
  assert.match(source, /idle\.className = "skin-choice-preview"/);
  assert.match(source, /label\.append\(idle\)/);
  assert.match(source, /details\.append\(reason\)/);
  const css = readFileSync(new URL("../src/styles/settings.css", import.meta.url), "utf8");
  assert.match(css, /\.skin-choice-preview\s*\{[^}]*object-fit: contain;/);
});