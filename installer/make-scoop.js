// Makes the Scoop manifest for a release, from its portable zip:
//
//   node installer/make-scoop.js [folder]
//
// reads UDM-<version>-windows-portable.zip in `folder` (dist by default)
// and writes udm.json beside it, which the Release workflow attaches to the
// release. `scoop install` takes it by its address on the latest release
// (docs/install.md), and Scoop reads it from there again on `scoop update`.
// Its `checkver` and `autoupdate` let a Scoop bucket carry the same file.
//
// Scoop unpacks the zip into apps\udm\<version> and points apps\udm\current
// at it. The manifest removes portable.txt, so the copy keeps its settings
// and list in the usual folders as Setup's copy does (docs/portable.md),
// and a new version finds them. It then registers the browsers with
// install-host.ps1, as Setup does, and removes them again on
// `scoop uninstall` only: Scoop runs the uninstaller on `scoop update` too,
// with $cmd set to "update". UDM tells a Scoop copy by Scoop's files beside
// it and leaves its updates to Scoop (update_in_place::this_install).

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const REPOSITORY = process.env.GITHUB_REPOSITORY || "tajbinkhan/UDM";
const ZIP = /^UDM-(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)-windows-portable\.zip$/;

// The manifest for `version`, whose zip has the SHA-256 `hash`.
function manifest(version, hash) {
  const home = `https://github.com/${REPOSITORY}`;
  const zip = (v) => `${home}/releases/download/v${v}/UDM-${v}-windows-portable.zip`;
  const host = "& \"$dir\\install-host.ps1\"";
  return {
    version,
    description: "Download manager that catches downloads from Chrome, Edge and Firefox",
    homepage: home,
    license: "MIT",
    notes: [
      "Load the browser extension once from $dir\\extension (Firefox: $dir\\firefox), as",
      `${home}/blob/main/docs/install.md#windows says.`,
      "Quit UDM from the tray before `scoop update udm`.",
    ],
    architecture: {
      "64bit": { url: zip(version), hash },
    },
    extract_dir: "UDM",
    pre_install: "Remove-Item \"$dir\\portable.txt\" -ErrorAction SilentlyContinue",
    post_install: `${host} -NoShortcut`,
    uninstaller: { script: `if ($cmd -eq 'uninstall') { ${host} -Uninstall }` },
    bin: [["target\\release\\udm-gui.exe", "udm"], "target\\release\\udm-cli.exe", "target\\release\\udm-mcp.exe"],
    shortcuts: [["target\\release\\udm-gui.exe", "UDM"]],
    checkver: "github",
    autoupdate: {
      architecture: {
        "64bit": { url: zip("$version") },
      },
      hash: { url: "$baseurl/SHA256SUMS" },
    },
  };
}

function main() {
  const dir = path.resolve(process.argv[2] || "dist");
  const zips = fs.readdirSync(dir).filter((name) => ZIP.test(name));
  if (zips.length !== 1) {
    console.error(`make-scoop: expected one portable zip in ${dir}, found ${zips.length}`);
    process.exit(1);
  }
  const [name] = zips;
  const version = ZIP.exec(name)[1];
  const hash = crypto.createHash("sha256").update(fs.readFileSync(path.join(dir, name))).digest("hex");
  const out = path.join(dir, "udm.json");
  fs.writeFileSync(out, JSON.stringify(manifest(version, hash), null, 2) + "\n");
  console.log(`make-scoop: wrote ${out} for ${name}`);
}

if (require.main === module) main();

module.exports = { manifest };
