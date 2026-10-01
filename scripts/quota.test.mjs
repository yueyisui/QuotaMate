import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import ts from "typescript";

// Transpile the real selector with the project's existing compiler: no test
// dependency and no assumption that the host Node supports stripping TS.
const source = readFileSync(new URL("../src/services/quota.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } });
const { quotaWindows, quotaShortLabel } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
const window = (windowMinutes, remainingPercent = 80) => ({ windowMinutes, remainingPercent, usedPercent: 100 - remainingPercent, resetAt: null });
const snapshot = (fiveHour = null, weekly = null, otherWindows = []) => ({ fiveHour, weekly, otherWindows });

test("both windows keep their original order and labels", () => {
  const usage = snapshot(window(300), window(10080));
  assert.deepEqual(quotaWindows(usage).map(quotaShortLabel), ["5h", "W"]);
  assert.equal(quotaWindows(usage, true, false).length, 1);
  assert.equal(quotaWindows(usage, false, true)[0].windowMinutes, 10080);
});
test("weekly-only accounts do not render an empty 5h slot", () => {
  const usage = snapshot(null, window(10080, 97));
  assert.deepEqual(quotaWindows(usage).map(quotaShortLabel), ["W"]);
  assert.equal(quotaWindows(usage, true, false)[0].remainingPercent, 97);
});
test("five-hour-only accounts do not render an empty weekly slot", () => {
  assert.deepEqual(quotaWindows(snapshot(window(300)), false, true).map(quotaShortLabel), ["5h"]);
});
test("missing all windows does not invent 100% or 0%", () => {
  assert.deepEqual(quotaWindows(snapshot()), []);
});
test("explicitly hiding both windows is preserved", () => {
  assert.deepEqual(quotaWindows(snapshot(window(300), window(10080)), false, false), []);
});
test("preferences survive account switches", () => {
  const plus = snapshot(window(300), window(10080));
  const weeklyOnly = snapshot(null, window(10080));
  assert.equal(quotaWindows(plus, true, false)[0].windowMinutes, 300);
  assert.equal(quotaWindows(weeklyOnly, true, false)[0].windowMinutes, 10080);
  assert.equal(quotaWindows(plus, true, false)[0].windowMinutes, 300);
});
test("other durations use their real labels", () => {
  assert.deepEqual(quotaWindows(snapshot(null, null, [window(1440), window(15)])).map(quotaShortLabel), ["24h", "15m"]);
});
test("older payloads without otherWindows remain compatible", () => {
  assert.deepEqual(quotaWindows({ fiveHour: null, weekly: window(10080) }).map(quotaShortLabel), ["W"]);
});

// Exercise the actual pet component and its energy logic as well as selection.
const dataUrl = (code) => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const compile = (file) => ts.transpileModule(readFileSync(new URL(file, import.meta.url), "utf8"), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2020, jsx: ts.JsxEmit.ReactJSX },
}).outputText;
const require = createRequire(import.meta.url);
const petCode = compile("../src/components/PetAvatar.tsx")
  .replaceAll('"../i18n"', JSON.stringify(dataUrl(compile("../src/i18n.ts"))))
  .replaceAll('"../services/quota"', JSON.stringify(dataUrl(outputText)))
  .replaceAll('"react/jsx-runtime"', JSON.stringify(pathToFileURL(require.resolve("react/jsx-runtime")).href));
const { usageEnergy, energyState, PetAvatar } = await import(dataUrl(petCode));
test("pet energy falls back to weekly and missing data stays neutral", () => {
  assert.equal(usageEnergy(snapshot(null, window(10080, 97))), 97);
  assert.equal(usageEnergy(snapshot(window(300, 0), window(10080, 97))), 0);
  assert.equal(usageEnergy(snapshot()), null);
  assert.equal(energyState(null, "zh-CN").key, "unknown");
  assert.equal(energyState(0, "zh-CN").key, "empty");
});
test("unknown pet state does not announce an exhausted quota", () => {
  const html = renderToStaticMarkup(createElement(PetAvatar, { preset: "cat", energy: null, language: "zh-CN" }));
  assert.match(html, /pet-svg--unknown/);
  assert.match(html, /等待数据/);
  assert.doesNotMatch(html, /需要充能/);
});
