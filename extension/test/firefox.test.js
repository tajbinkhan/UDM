// installer/make-firefox.js's --check, which tells whether a Firefox copy of
// the extension is still what the extension makes (#149: a copy made before
// a change silently lacked it).

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { makeFirefox, checkFirefox } = require("../../installer/make-firefox.js");

// A fresh Firefox copy in a new temporary folder, removed after the test.
function freshCopy(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "udm-firefox-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const out = path.join(dir, "firefox");
  makeFirefox(out);
  return out;
}

test("a fresh Firefox copy passes the check and holds every extension script", (t) => {
  const out = freshCopy(t);
  assert.deepEqual(checkFirefox(out), []);
  for (const file of ["background.js", "media.js", "rules.js", "links.js", "post.js", "request.js", "content.js", "popup.js"]) {
    assert.ok(fs.readFileSync(path.join(__dirname, "..", file)).equals(fs.readFileSync(path.join(out, file))), file);
  }
  assert.ok(fs.readFileSync(path.join(out, "media.js"), "utf8").includes("function adoptFile("));
  assert.equal(fs.existsSync(path.join(out, "test")), false);
  assert.equal(fs.existsSync(path.join(out, "firefox.json")), false);
});

test("the check finds a changed file, as in a copy older than the extension", (t) => {
  const out = freshCopy(t);
  const media = path.join(out, "media.js");
  fs.writeFileSync(media, fs.readFileSync(media, "utf8").replace("function adoptFile(", "function oldName("));
  assert.deepEqual(checkFirefox(out), ["changed: media.js"]);
});

test("the check finds a missing file and an extra one, in subfolders too", (t) => {
  const out = freshCopy(t);
  fs.rmSync(path.join(out, "icons", "16.png"));
  fs.writeFileSync(path.join(out, "old.js"), "");
  fs.mkdirSync(path.join(out, "test"));
  fs.writeFileSync(path.join(out, "test", "media.test.js"), "");
  assert.deepEqual(checkFirefox(out), ["missing: icons/16.png", "extra: old.js", "extra: test/media.test.js"]);
});

test("the check compares manifest.json as JSON, not as text", (t) => {
  const out = freshCopy(t);
  const file = path.join(out, "manifest.json");
  const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
  // Other spacing, key order, escapes and a byte order mark, as PowerShell
  // may write it, are the same manifest.
  const reordered = Object.fromEntries(Object.entries(manifest).reverse());
  fs.writeFileSync(file, "﻿" + JSON.stringify(reordered).replace(/</g, "\\u003c"));
  assert.deepEqual(checkFirefox(out), []);

  // A changed, a missing and an added key are not.
  manifest.background = { service_worker: "background.js" };
  delete manifest.browser_specific_settings;
  manifest.key = "abc";
  fs.writeFileSync(file, JSON.stringify(manifest));
  assert.deepEqual(checkFirefox(out), [
    'changed: manifest.json key "background"',
    'changed: manifest.json key "browser_specific_settings"',
    'changed: manifest.json key "key"',
  ]);

  fs.writeFileSync(file, "{");
  assert.match(checkFirefox(out).join(), /^changed: manifest\.json is not JSON/);
  fs.rmSync(file);
  assert.deepEqual(checkFirefox(out), ["missing: manifest.json"]);
});

test("the check fails for a folder that does not exist", (t) => {
  const out = freshCopy(t);
  fs.rmSync(out, { recursive: true });
  assert.equal(checkFirefox(out).length, 1);
});

test("making a copy never replaces a folder that holds no extension", (t) => {
  const out = freshCopy(t);
  fs.rmSync(path.join(out, "manifest.json"));
  assert.throws(() => makeFirefox(out), /holds no extension/);
  assert.ok(fs.existsSync(path.join(out, "media.js")));
});
