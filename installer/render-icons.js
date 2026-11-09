// Renders assets/icon.svg into every icon file Pullbay ships:
// extension/icons/{16,32,48,128}.png, assets/icon-256.png, assets/icon-1024.png
// and assets/icon-64.rgba (the window and tray icon, raw RGBA). Run
// make-icon.js afterwards to pack installer/pullbay.ico.
//
// It draws the SVG on a canvas in headless Chrome. Set CHROME to the browser
// when it is not in its usual Windows place.
//
//   node installer/render-icons.js

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const root = path.join(__dirname, "..");
const chrome =
  process.env.CHROME || "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const svg = fs.readFileSync(path.join(root, "assets", "icon.svg"), "utf8");
const sizes = [16, 32, 48, 64, 128, 256, 1024];

const page = `<!doctype html><body><script>
const img = new Image();
img.onload = () => {
  const out = {};
  for (const s of ${JSON.stringify(sizes)}) {
    const c = document.createElement("canvas");
    c.width = c.height = s;
    const g = c.getContext("2d");
    g.imageSmoothingQuality = "high";
    g.drawImage(img, 0, 0, s, s);
    out[s] = { png: c.toDataURL("image/png").split(",")[1] };
    if (s === 64) out[s].rgba = btoa(String.fromCharCode(...g.getImageData(0, 0, s, s).data));
  }
  const pre = document.createElement("pre");
  pre.id = "out";
  pre.textContent = JSON.stringify(out);
  document.body.appendChild(pre);
};
img.src = "data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}";
</script></body>`;

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pullbay-icon-"));
const html = path.join(dir, "render.html");
fs.writeFileSync(html, page);
const dom = execFileSync(
  chrome,
  ["--headless", "--disable-gpu", `--user-data-dir=${path.join(dir, "profile")}`,"--virtual-time-budget=5000", "--dump-dom", `file:///${html.replace(/\\/g, "/")}`],
  { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
);
fs.rmSync(dir, { recursive: true, force: true });

const match = dom.match(/<pre id="out">([^<]*)<\/pre>/);
if (!match) throw new Error("Chrome did not render the icon");
const out = JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"));

const write = (file, data) => {
  fs.writeFileSync(path.join(root, file), data);
  console.log(`wrote ${file} (${data.length} bytes)`);
};
for (const s of [16, 32, 48, 128]) write(`extension/icons/${s}.png`, Buffer.from(out[s].png, "base64"));
write("assets/icon-256.png", Buffer.from(out[256].png, "base64"));
write("assets/icon-1024.png", Buffer.from(out[1024].png, "base64"));
write("assets/icon-64.rgba", Buffer.from(out[64].rgba, "base64"));
