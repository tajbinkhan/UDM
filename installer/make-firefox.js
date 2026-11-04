// Makes the Firefox copy of the extension, as make-firefox.ps1 does on
// Windows, for the macOS and Linux packages:
//
//   node installer/make-firefox.js firefox
//
// The copy is the extension folder without its tests, with manifest.json
// changed by extension/firefox.json: each key there replaces the
// manifest's, and null removes it.
//
// The copy is made, not kept in git, so one made before the extension
// changed silently lacks the change (#149). This tells whether a copy is
// still what the extension makes, listing what differs, and exits with 1
// if it is not:
//
//   node installer/make-firefox.js --check firefox
//
// Every file must be byte for byte the same and none missing or extra.
// manifest.json is compared as JSON, since make-firefox.ps1 writes it with
// PowerShell's own spacing, key order and escapes.

const fs = require("node:fs");
const path = require("node:path");
const util = require("node:util");

const SOURCE = path.join(__dirname, "..", "extension");
// What the extension folder holds that the copy leaves out or remakes.
const SKIPPED = ["test", "firefox.json", "manifest.json"];

// The Firefox manifest made from `source`'s manifest.json and firefox.json.
function firefoxManifest(source = SOURCE) {
  const read = (file) => JSON.parse(fs.readFileSync(path.join(source, file), "utf8"));
  const manifest = read("manifest.json");
  for (const [key, value] of Object.entries(read("firefox.json"))) {
    delete manifest[key];
    if (value !== null) manifest[key] = value;
  }
  return manifest;
}

// Every file under `dir`, as paths relative to it with "/" between folders,
// sorted, leaving out the top-level names in `skip`.
function listFiles(dir, skip = []) {
  const files = [];
  const walk = (rel) => {
    for (const entry of fs.readdirSync(path.join(dir, rel), { withFileTypes: true })) {
      const name = rel ? `${rel}/${entry.name}` : entry.name;
      if (!rel && skip.includes(entry.name)) continue;
      if (entry.isDirectory()) walk(name);
      else files.push(name);
    }
  };
  walk("");
  return files.sort();
}

// Write the Firefox copy of `source` to `out`, replacing an earlier copy.
function makeFirefox(out, source = SOURCE) {
  if (fs.existsSync(out)) {
    // Only ever replace an earlier copy, never some other folder.
    if (fs.readdirSync(out).length && !fs.existsSync(path.join(out, "manifest.json"))) {
      throw new Error(`${out} is not empty and holds no extension`);
    }
    fs.rmSync(out, { recursive: true, force: true });
  }
  fs.mkdirSync(out, { recursive: true });
  for (const name of fs.readdirSync(source)) {
    if (SKIPPED.includes(name)) continue;
    fs.cpSync(path.join(source, name), path.join(out, name), { recursive: true });
  }
  fs.writeFileSync(path.join(out, "manifest.json"), JSON.stringify(firefoxManifest(source), null, 2) + "\n");
}

// How the Firefox copy in `folder` differs from what `source` makes, one
// line per difference; none when it is up to date.
function checkFirefox(folder, source = SOURCE) {
  if (!fs.existsSync(folder)) return [`${folder} does not exist`];
  const differences = [];
  const expected = listFiles(source, SKIPPED);
  const found = new Set(listFiles(folder));
  for (const file of expected) {
    if (!found.has(file)) differences.push(`missing: ${file}`);
    else if (!fs.readFileSync(path.join(source, file)).equals(fs.readFileSync(path.join(folder, file)))) {
      differences.push(`changed: ${file}`);
    }
    found.delete(file);
  }
  if (!found.has("manifest.json")) {
    differences.push("missing: manifest.json");
  } else {
    found.delete("manifest.json");
    let manifest;
    try {
      manifest = JSON.parse(fs.readFileSync(path.join(folder, "manifest.json"), "utf8").replace(/^﻿/, ""));
    } catch (e) {
      manifest = undefined;
      differences.push(`changed: manifest.json is not JSON (${e.message})`);
    }
    if (manifest !== undefined) {
      const want = firefoxManifest(source);
      const keys = [...new Set([...Object.keys(want), ...Object.keys(manifest)])].sort();
      for (const key of keys) {
        if (!util.isDeepStrictEqual(want[key], manifest[key])) differences.push(`changed: manifest.json key "${key}"`);
      }
    }
  }
  for (const file of [...found].sort()) differences.push(`extra: ${file}`);
  return differences;
}

module.exports = { makeFirefox, checkFirefox, firefoxManifest };

if (require.main === module) {
  const args = process.argv.slice(2);
  const checking = args[0] === "--check";
  const folder = checking ? args[1] : args[0];
  if (!folder || args.length !== (checking ? 2 : 1)) {
    console.error("usage: node installer/make-firefox.js [--check] <folder>");
    process.exit(2);
  }
  if (checking) {
    const differences = checkFirefox(folder);
    if (differences.length) {
      console.error(`${folder} is not the Firefox copy of the current extension:`);
      for (const d of differences) console.error(`  ${d}`);
      console.error(`Remake it with: node installer/make-firefox.js ${folder}`);
      process.exit(1);
    }
    console.log(`${folder} is up to date with the extension`);
  } else {
    try {
      makeFirefox(folder);
    } catch (e) {
      console.error(e.message);
      process.exit(1);
    }
    console.log(`Wrote the Firefox extension to ${folder}`);
  }
}
