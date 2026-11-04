// Tests for what "Download all links" and "Download selected links" collect
// from a page (links.js, #155): links, frames, inline scripts, images'
// onclick handlers, text selected in a text box, images in a selection and
// frames made from nothing, each address once. The page is a small fake
// document with just the DOM calls links.js makes. Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const {
  resolveLink,
  linksFromText,
  linksFromScriptText,
  linksFromOnclick,
  bestSrcset,
  dedupeLinks,
  collectPageLinks,
  selectionHasPageLinks,
  LINKS_SCRIPT_BYTES,
} = require("../links.js");

// An element: its tag, attributes, text and (for a frame) its document.
function el(tag, attrs = {}, extra = {}) {
  return {
    tagName: tag.toUpperCase(),
    getAttribute: (n) => (n in attrs ? attrs[n] : null),
    textContent: "",
    ...extra,
  };
}

// A document at `base` holding `elements`; `selected` is what the selection
// touches (with `text` its text), none when it is null.
function doc(base, elements, { selected = null, text = "", active = null } = {}) {
  const by = {
    "a[href], area[href]": (e) => /^(A|AREA)$/.test(e.tagName) && e.getAttribute("href") != null,
    "iframe, frame": (e) => /^(IFRAME|FRAME)$/.test(e.tagName),
    script: (e) => e.tagName === "SCRIPT",
    img: (e) => e.tagName === "IMG",
  };
  return {
    baseURI: base,
    activeElement: active,
    querySelectorAll: (s) => elements.filter(by[s]),
    getSelection: () =>
      selected
        ? { isCollapsed: false, rangeCount: 1, toString: () => text, getRangeAt: () => ({ intersectsNode: (n) => selected.includes(n) }) }
        : { isCollapsed: true, rangeCount: 0, toString: () => "" },
  };
}

const urls = (links) => links.map((l) => l.url);

test("addresses are made absolute, web only, without their fragment", () => {
  assert.equal(resolveLink("../b.zip#part", "https://site.net/a/page"), "https://site.net/b.zip");
  assert.equal(resolveLink("ftp://files.net/x.iso"), "ftp://files.net/x.iso");
  assert.equal(resolveLink("javascript:void(0)", "https://site.net/"), "");
  assert.equal(resolveLink("mailto:a@b.net", "https://site.net/"), "");
  assert.equal(resolveLink("", "https://site.net/"), "");
  assert.equal(resolveLink("x.zip"), "", "a relative address without a base is none");
});

test("addresses in text, as a selection holds them", () => {
  assert.deepEqual(linksFromText("see https://a.net/x.zip, and (ftp://b.net/y.iso). Not www.c.net", ""), [
    "https://a.net/x.zip",
    "ftp://b.net/y.iso",
  ]);
});

test("addresses in an inline script: absolute ones, and relative files of a known type", () => {
  const code = `
    var cdn = "https:\\/\\/cdn.net\\/setup.exe";
    load('/files/report.pdf?v=2'); open("data/archive.ZIP");
    route("/users/list"); css('main.css'); s = "jquery.min.js"; t = "a.mkv2";
    fetch("//mirror.org/movie.mkv"); u = 'custom.abc';
  `;
  assert.deepEqual(linksFromScriptText(code, "https://site.net/app/", ["abc"]), [
    "https://cdn.net/setup.exe",
    "https://site.net/files/report.pdf?v=2",
    "https://site.net/app/data/archive.ZIP",
    "https://mirror.org/movie.mkv",
    "https://site.net/app/custom.abc",
  ]);
  assert.deepEqual(linksFromScriptText("x = 'custom.abc'", "https://site.net/"), [], "a type UIDM does not capture is junk");
});

test("an image's onclick handler gives its picture", () => {
  assert.deepEqual(linksFromOnclick("window.open('big/photo.jpg','_blank')", "https://site.net/g/"), ["https://site.net/g/big/photo.jpg"]);
  assert.deepEqual(linksFromOnclick("show('http://img.net/a.png')", "https://site.net/"), ["http://img.net/a.png"]);
  assert.deepEqual(linksFromOnclick("toggle(this)", "https://site.net/"), []);
});

test("a srcset gives its largest picture", () => {
  assert.equal(bestSrcset("s.jpg 1x, l.jpg 2x", "https://site.net/"), "https://site.net/l.jpg");
  assert.equal(bestSrcset("a.jpg 480w, b.jpg 1080w, c.jpg 800w", "https://site.net/"), "https://site.net/b.jpg");
  assert.equal(bestSrcset("", "https://site.net/"), "");
});

test("links are listed once, first seen first, with the best text", () => {
  const links = dedupeLinks([
    { url: "https://a.net/x.zip#1", text: "" },
    { url: "https://b.net/y.png", text: "", image: true },
    { url: "https://a.net/x.zip#2", text: "Get X" },
    { url: "https://b.net/y.png", text: "Y", image: false },
    { url: "javascript:void(0)", text: "no" },
    { url: "https://c.net/z", text: "z".repeat(500) },
  ]);
  assert.deepEqual(links, [
    { url: "https://a.net/x.zip", text: "Get X" },
    { url: "https://b.net/y.png", text: "Y" },
    { url: "https://c.net/z", text: "z".repeat(200) },
  ]);
  assert.equal(dedupeLinks([{ url: "https://a.net/1" }, { url: "https://a.net/2" }, { url: "https://a.net/1", text: "t" }], 1).length, 1);
});

test("all links: links, frames, inline scripts, onclick handlers and images, each once", () => {
  const page = doc("https://site.net/dir/", [
    el("a", { href: "a.zip" }, { textContent: " Get  A " }),
    el("area", { href: "/map.pdf", alt: "Map" }),
    el("a", { href: "a.zip#again" }),
    el("a", { href: "javascript:go()" }),
    el("iframe", { src: "https://player.net/embed/1", title: "Player" }),
    el("frame", { src: "nav.html" }),
    el("script", {}, { textContent: "var u = 'https://cdn.net/b.exe'; get('files/c.7z');" }),
    el("script", { src: "app.js" }, { textContent: "'https://never.net/x.zip'" }),
    el("img", { src: "thumb.jpg", alt: "Photo", onclick: "popup('large.jpg')", srcset: "thumb.jpg 1x, thumb@2x.jpg 2x" }),
  ]);
  assert.deepEqual(collectPageLinks(page), [
    { url: "https://site.net/dir/a.zip", text: "Get A" },
    { url: "https://site.net/map.pdf", text: "Map" },
    { url: "https://player.net/embed/1", text: "Player" },
    { url: "https://site.net/dir/nav.html", text: "" },
    { url: "https://cdn.net/b.exe", text: "" },
    { url: "https://site.net/dir/files/c.7z", text: "" },
    { url: "https://site.net/dir/large.jpg", text: "Photo" },
    { url: "https://site.net/dir/thumb.jpg", text: "Photo", image: true },
    { url: "https://site.net/dir/thumb@2x.jpg", text: "Photo", image: true },
  ]);
});

test("the script text read is capped", () => {
  const filler = "x".repeat(LINKS_SCRIPT_BYTES);
  const page = doc("https://site.net/", [
    el("script", {}, { textContent: "'https://cdn.net/first.zip'" }),
    el("script", {}, { textContent: filler + "'https://cdn.net/late.zip'" }),
  ]);
  assert.deepEqual(urls(collectPageLinks(page)), ["https://cdn.net/first.zip"]);
});

test("frames made from nothing are walked into, against their own base, and others' are not", () => {
  const deepest = doc("https://site.net/d4/", [el("a", { href: "too-deep.zip" })]);
  const level3 = doc("https://site.net/d3/", [el("a", { href: "c.zip" }), el("iframe", {}, { contentDocument: deepest })]);
  const level2 = doc("https://site.net/d2/", [el("a", { href: "b.zip" }), el("iframe", { src: "about:blank" }, { contentDocument: level3 })]);
  const inner = doc("https://site.net/inner/", [el("a", { href: "in.zip" }), el("a", { href: "/top.zip" }), el("iframe", { srcdoc: "<p>" }, { contentDocument: level2 })]);
  const other = el("iframe", {});
  Object.defineProperty(other, "contentDocument", {
    get() {
      throw new Error("SecurityError");
    },
  });
  const page = doc("https://site.net/", [
    el("a", { href: "/top.zip" }),
    el("iframe", {}, { contentDocument: inner }),
    other,
    el("iframe", { src: "https://ads.net/" }, { contentDocument: null }),
  ]);
  assert.deepEqual(urls(collectPageLinks(page)), [
    "https://site.net/top.zip",
    "https://ads.net/",
    "https://site.net/inner/in.zip",
    "https://site.net/d2/b.zip",
    "https://site.net/d3/c.zip",
  ]);
});

test("selected links: what the selection touches, its addresses and its images", () => {
  const inside = el("a", { href: "in.zip" }, { textContent: "In" });
  const outside = el("a", { href: "out.zip" });
  const image = el("img", { src: "pic.png", alt: "Pic" });
  const otherImage = el("img", { src: "other.png" });
  const frame = el("iframe", { src: "https://player.net/1" });
  const script = el("script", {}, { textContent: "'https://cdn.net/s.exe'" });
  const elements = [inside, outside, image, otherImage, frame, el("iframe", { src: "https://ads.net/" }), script, el("script", {}, { textContent: "'https://cdn.net/no.exe'" })];
  const page = doc("https://site.net/", elements, { selected: [inside, image, frame, script], text: "In and ftp://files.net/a.iso" });
  assert.deepEqual(urls(collectPageLinks(page, { selection: true })), [
    "ftp://files.net/a.iso",
    "https://site.net/in.zip",
    "https://player.net/1",
    "https://cdn.net/s.exe",
    "https://site.net/pic.png",
  ]);
  assert.equal(selectionHasPageLinks(page), true);

  // Nothing selected: nothing.
  const none = doc("https://site.net/", elements);
  assert.deepEqual(collectPageLinks(none, { selection: true }), []);
  assert.equal(selectionHasPageLinks(none), false);

  // Only an image selected still counts, as Firefox's menu then shows.
  assert.equal(selectionHasPageLinks(doc("https://site.net/", elements, { selected: [otherImage], text: "" })), true);
  // Text without links does not.
  assert.equal(selectionHasPageLinks(doc("https://site.net/", elements, { selected: [], text: "just words" })), false);
});

test("selected links: the text selected in a text box", () => {
  const box = el("textarea", {}, { value: "skip https://a.net/x.zip take https://b.net/y.zip and https://c.net/z.zip", selectionStart: 25, selectionEnd: 74 });
  const page = doc("https://site.net/", [box], { active: box });
  assert.deepEqual(urls(collectPageLinks(page, { selection: true })), ["https://b.net/y.zip", "https://c.net/z.zip"]);
  assert.equal(selectionHasPageLinks(page), true);

  // A box with nothing selected, or a kind of input that has no text
  // selection, gives nothing.
  box.selectionEnd = 25;
  assert.equal(selectionHasPageLinks(page), false);
  const check = el("input", {});
  Object.defineProperty(check, "selectionStart", {
    get() {
      throw new Error("InvalidStateError");
    },
  });
  assert.equal(selectionHasPageLinks(doc("https://site.net/", [check], { active: check })), false);
});

test("a selection in a frame made from nothing is its own", () => {
  const link = el("a", { href: "in-frame.zip" });
  const inner = doc("https://site.net/f/", [link, el("a", { href: "not.zip" })], { selected: [link], text: "x" });
  const page = doc("https://site.net/", [el("a", { href: "top.zip" }), el("iframe", {}, { contentDocument: inner })]);
  assert.deepEqual(urls(collectPageLinks(page, { selection: true })), ["https://site.net/f/in-frame.zip"]);
});
