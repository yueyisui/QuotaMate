import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("../src/services/configSaveQueue.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } });
const { createConfigSaveQueue } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const initial = { petPreset: "cat", petScale: 1, opacity: 1, petPosition: { x: 10, y: 20 } };

function fixture(write, read) {
  let stored = structuredClone(initial);
  let visible;
  const errors = [];
  const queue = createConfigSaveQueue(
    read ?? (async () => structuredClone(stored)),
    async (next) => { const saved = await write(next); stored = structuredClone(saved); return saved; },
    (next) => { visible = next; },
    (reason) => errors.push(reason),
  );
  queue.receive(structuredClone(stored));
  return { queue, errors, visible: () => visible, stored: () => stored,
    store: (next) => { stored = structuredClone(next); } };
}

test("rapid pet and slider patches coalesce and retain every latest field", async () => {
  const writes = [];
  const f = fixture(async (next) => { writes.push(next); return next; });
  const first = f.queue.patch({ petPreset: "dog" });
  f.queue.patch({ petScale: 0.7 });
  f.queue.patch({ opacity: 0.65, petPreset: "tiga" });
  assert.deepEqual(f.visible(), { ...initial, petPreset: "tiga", petScale: 0.7, opacity: 0.65 });
  await first;
  assert.equal(writes.length, 1);
  assert.deepEqual(f.stored(), f.visible());
});

test("older events and replies cannot replace a newer selection during saving", async () => {
  const firstWrite = deferred();
  const writes = [];
  let concurrent = 0;
  let maximum = 0;
  const f = fixture(async (next) => {
    maximum = Math.max(maximum, ++concurrent);
    writes.push(next);
    if (writes.length === 1) await firstWrite.promise;
    --concurrent;
    return next;
  });
  const completion = f.queue.patch({ petPreset: "dog" });
  await tick();
  f.queue.patch({ petPreset: "tiga" });
  f.queue.patch({ opacity: 0.35 });
  f.queue.receive({ ...initial, petPreset: "dog" });
  assert.equal(f.visible().petPreset, "tiga");
  firstWrite.resolve();
  await completion;
  assert.equal(maximum, 1);
  assert.equal(writes.length, 2);
  assert.equal(f.visible().petPreset, "tiga");
  assert.equal(f.stored().opacity, 0.35);
});

test("a fresh backend base preserves native position changes between writes", async () => {
  const firstWrite = deferred();
  const secondRead = deferred();
  let reads = 0;
  let writes = 0;
  let stored = structuredClone(initial);
  let visible;
  const queue = createConfigSaveQueue(async () => {
    if (++reads === 2) await secondRead.promise;
    return structuredClone(stored);
  }, async (next) => {
    if (++writes === 1) await firstWrite.promise;
    stored = next;
    return next;
  }, (next) => { visible = next; }, () => {});
  queue.receive(initial);
  const completion = queue.patch({ petPreset: "dog" });
  await tick();
  queue.patch({ opacity: 0.65 });
  firstWrite.resolve();
  await tick();
  stored = { ...stored, petPosition: { x: 500, y: 400 } };
  secondRead.resolve();
  await completion;
  assert.deepEqual(visible.petPosition, { x: 500, y: 400 });
  assert.equal(visible.opacity, 0.65);
});

test("new edits arriving during final reconciliation are also saved", async () => {
  const reconciliation = deferred();
  let reads = 0;
  let stored = structuredClone(initial);
  const queue = createConfigSaveQueue(async () => {
    if (++reads === 2) await reconciliation.promise;
    return stored;
  }, async (next) => { stored = next; return next; }, () => {}, () => {});
  queue.receive(initial);
  const completion = queue.patch({ petPreset: "robot" });
  await tick();
  queue.patch({ petScale: 1.5 });
  reconciliation.resolve();
  await completion;
  assert.equal(stored.petScale, 1.5);
  assert.equal(stored.petPreset, "robot");
});

test("a failed earlier write rebases and retries newer pending edits once", async () => {
  const failure = deferred();
  const writes = [];
  const f = fixture(async (next) => {
    writes.push(next);
    if (writes.length === 1) await failure.promise;
    return next;
  });
  const completion = f.queue.patch({ petPreset: "car" });
  await tick();
  f.queue.patch({ petScale: 0.7 });
  failure.reject(new Error("temporary write failure"));
  await completion;
  assert.equal(writes.length, 2);
  assert.equal(f.stored().petPreset, "car");
  assert.equal(f.stored().petScale, 0.7);
  assert.deepEqual(f.errors, [null]);
});

test("persistent failure restores backend state and a later edit can succeed", async () => {
  let failing = true;
  const f = fixture(async (next) => {
    if (failing) throw new Error("cannot save");
    return next;
  });
  await assert.rejects(f.queue.patch({ opacity: 0.35 }), /cannot save/);
  assert.deepEqual(f.visible(), initial);
  failing = false;
  await f.queue.patch({ petPreset: "rocket" });
  assert.equal(f.visible().petPreset, "rocket");
  assert.equal(f.visible().opacity, 1);
  assert.equal(String(f.errors[0]), "Error: cannot save");
});

test("read failure restores the confirmed snapshot rather than unsaved optimistic edits", async () => {
  const f = fixture(async (next) => next, async () => { throw new Error("backend unavailable"); });
  await assert.rejects(f.queue.patch({ petPreset: "dog" }), /backend unavailable/);
  assert.deepEqual(f.visible(), initial);
});

test("idle external changes become the base of the next optimistic patch", async () => {
  const f = fixture(async (next) => next);
  const external = { ...initial, petPreset: "rocket", opacity: 0.35 };
  f.store(external);
  f.queue.receive(external);
  const completion = f.queue.patch({ petScale: 1.5 });
  assert.equal(f.visible().opacity, 0.35);
  await completion;
  assert.deepEqual(f.visible(), { ...external, petScale: 1.5 });
});
