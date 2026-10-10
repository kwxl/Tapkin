import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { setTimeout as wait } from "node:timers/promises";
import ts from "typescript";

const source = readFileSync(new URL("../src/shared/autosave.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
});
const { createAutosave } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);

test("debounces edits and saves only the latest value", async () => {
  const writes = [];
  let saved = 0;
  const autosave = createAutosave({
    delay: 10,
    save: async (value) => { writes.push(value); },
    saved: () => { saved += 1; },
    failed: assert.fail,
  });
  autosave.update(100);
  autosave.update(200);
  assert.deepEqual(writes, []);
  await wait(40);
  assert.deepEqual(writes, [200]);
  assert.equal(saved, 1);
});

test("serializes in-flight writes and suppresses stale completion", async () => {
  const writes = [];
  const resolvers = [];
  let saved = 0;
  const autosave = createAutosave({
    delay: 10,
    save: (value) => {
      writes.push(value);
      return new Promise((resolve) => resolvers.push(resolve));
    },
    saved: () => { saved += 1; },
    failed: assert.fail,
  });
  autosave.update(100);
  const first = autosave.flush();
  autosave.update(200);
  await autosave.flush();
  assert.deepEqual(writes, [100]);
  resolvers.shift()();
  await first;
  assert.deepEqual(writes, [100, 200]);
  assert.equal(saved, 0);
  resolvers.shift()();
  await wait(0);
  assert.equal(saved, 1);
});

test("invalid edits cancel pending saves and invalidate in-flight success", async () => {
  const writes = [];
  let complete;
  let saved = 0;
  const autosave = createAutosave({
    delay: 10,
    save: (value) => {
      writes.push(value);
      return new Promise((resolve) => { complete = resolve; });
    },
    saved: () => { saved += 1; },
    failed: assert.fail,
  });
  autosave.update(100);
  autosave.cancel();
  await wait(40);
  assert.deepEqual(writes, []);
  autosave.update(200);
  const request = autosave.flush();
  autosave.cancel();
  complete();
  await request;
  assert.equal(saved, 0);
});

test("reports save errors and accepts a subsequent retry", async () => {
  const failure = new Error("save failed");
  const failures = [];
  let saved = 0;
  const autosave = createAutosave({
    delay: 10,
    save: async (value) => { if (value === 100) throw failure; },
    saved: () => { saved += 1; },
    failed: (error) => failures.push(error),
  });
  autosave.update(100);
  await autosave.flush();
  assert.deepEqual(failures, [failure]);
  autosave.update(200);
  await autosave.flush();
  assert.equal(saved, 1);
});