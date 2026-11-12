// Tests for downloads started by a form POST (post.js), and for the service
// worker sending them as the same POST. Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { encodeForm, encodeMultipart, rebuildPost, findPost, postById, prunePosts, POST_MATCH_MS } = require("../post.js");
const { loadWorker, download, setBrowserProxy } = require("./harness.js");

const b64 = (s) => Buffer.from(s, "utf8").toString("base64");
const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));

test("forms encode as the browser sends them", () => {
  assert.equal(encodeForm({ q: ["a b&c"], id: ["1", "2"], "é": ["ü"] }), "q=a+b%26c&id=1&id=2&%C3%A9=%C3%BC");
  assert.equal(encodeForm({}), "");
});

test("rebuilding a recorded request", () => {
  const form = rebuildPost({
    requestBody: { formData: { file: ["report"], token: ["x y"] } },
    contentType: "application/x-www-form-urlencoded; charset=UTF-8",
  });
  same([form.method, form.text, form.contentType], ["POST", "file=report&token=x+y", "application/x-www-form-urlencoded"]);
  assert.equal(form.body, b64("file=report&token=x+y"));

  const raw = rebuildPost({
    requestBody: { raw: [{ bytes: new TextEncoder().encode('{"a":').buffer }, { bytes: new TextEncoder().encode("1}").buffer }] },
    contentType: "application/json",
  });
  assert.equal(raw.body, b64('{"a":1}'));
  assert.equal(raw.contentType, "application/json");
  assert.equal(raw.text, undefined, "the browser cannot resend raw bytes");

  // A multipart form, rebuilt with a boundary of its own and without its
  // file fields; one that sends a file stays in the browser.
  const multi = rebuildPost(
    {
      requestBody: { formData: { id: ["7"], note: ['a "b"'], upload: [""] } },
      contentType: "multipart/form-data; boundary=----WebKitFormBoundaryX",
      files: { names: ["upload"], chosen: false },
    },
    "B",
  );
  assert.equal(multi.contentType, "multipart/form-data; boundary=B");
  assert.equal(
    multi.text,
    '--B\r\nContent-Disposition: form-data; name="id"\r\n\r\n7\r\n--B\r\nContent-Disposition: form-data; name="note"\r\n\r\na "b"\r\n--B--\r\n',
  );
  assert.equal(multi.body, b64(multi.text));
  assert.ok(rebuildPost({ requestBody: { formData: { a: ["1"] } }, contentType: "multipart/form-data; boundary=x", files: { names: ["f"], chosen: true } }).leave);
  assert.match(rebuildPost({ requestBody: { formData: { a: ["1"] } }, contentType: "multipart/form-data; boundary=x" }).contentType, /^multipart\/form-data; boundary=----UdmFormBoundary[0-9a-f]{24}$/);

  // What cannot be rebuilt stays in the browser.
  assert.ok(rebuildPost({ requestBody: { formData: { a: ["1"] } }, contentType: "text/csv" }).leave);
  assert.ok(rebuildPost({ requestBody: { raw: [{ file: "C:\\secret.txt" }] } }).leave);
  assert.ok(rebuildPost({ requestBody: { error: "too big" } }).leave);
  assert.ok(rebuildPost({}).leave);
});

test("multipart fields are written as the browser writes them", () => {
  assert.equal(encodeMultipart({ 'a"\nb': ["x"] }, "Z"), '--Z\r\nContent-Disposition: form-data; name="a%22%0Ab"\r\n\r\nx\r\n--Z--\r\n');
  assert.equal(encodeMultipart({ a: ["1"], b: ["2"] }, "Z", ["a", "b"]), "--Z--\r\n");
});

test("a download matches the latest recent POST to its URL", () => {
  const now = 1_000_000;
  const records = [
    { url: "https://x/get", time: now - POST_MATCH_MS - 1, n: 1 },
    { url: "https://x/get", time: now - 5000, n: 2 },
    { url: "https://x/get", time: now - 1000, n: 3 },
    { url: "https://x/other", time: now - 500, n: 4 },
  ];
  assert.equal(findPost(records, "https://x/get", now).n, 3);
  assert.equal(findPost(records, "https://x/none", now), null);
  assert.equal(findPost(records, "https://x/get", now + POST_MATCH_MS), null, "too old");
  // A script's request matches only by its id.
  const xhr = [...records, { id: "x1", url: "https://x/api", type: "xmlhttprequest", time: now }];
  assert.equal(findPost(xhr, "https://x/api", now), null);
  assert.equal(postById(xhr, "x1").url, "https://x/api");
  prunePosts(records, now);
  assert.equal(records.length, 3);
});

function connected() {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: [], capture_min_size_kb: 0, capture_excluded_sites: [] } });
  return w;
}

function submit(w, { requestId, url, requestBody, contentType }) {
  w.chrome.webRequest.onBeforeRequest.fire({ requestId, url, method: "POST", type: "main_frame", requestBody });
  w.chrome.webRequest.onBeforeSendHeaders.fire({
    requestId,
    url,
    method: "POST",
    requestHeaders: contentType ? [{ name: "Content-Type", value: contentType }] : [],
  });
}

test("a form's download is handed to UDM as the same POST", async () => {
  const w = connected();
  submit(w, {
    requestId: "1",
    url: "https://site.net/export",
    requestBody: { formData: { report: ["q3"], fmt: ["pdf"] } },
    contentType: "application/x-www-form-urlencoded",
  });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/export", finalUrl: "https://cdn.site.net/r.pdf" }));
  const add = w.sent.find((m) => m.op === "add");
  same(
    [add.url, add.method, Buffer.from(add.body, "base64").toString(), add.headers["Content-Type"]],
    ["https://site.net/export", "POST", "report=q3&fmt=pdf", "application/x-www-form-urlencoded"],
  );

  // A plain link download is still a GET.
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/file.zip" }));
  const get = w.sent.filter((m) => m.op === "add").at(-1);
  same([get.url, get.method, get.body], ["https://site.net/file.zip", undefined, undefined]);
});

test("a form that cannot be resent stays in the browser", async () => {
  const w = connected();
  // Its page says it sends a file, which the browser does not share.
  await w.content({ type: "form:files", url: "https://site.net/upload", names: ["doc"], chosen: true }, 4, "https://site.net/form");
  submit(w, {
    requestId: "2",
    url: "https://site.net/upload",
    requestBody: { formData: { a: ["1"], doc: ["cv.pdf"] } },
    contentType: "multipart/form-data; boundary=----x",
  });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/upload" }));
  assert.equal(w.sent.filter((m) => m.op === "add").length, 0);
  assert.equal(w.browserCalls.filter((c) => c[0] === "cancel").length, 0, "the browser keeps it");
});

test("a multipart form's download is resent without its empty file field", async () => {
  const w = connected();
  await w.content({ type: "form:files", url: "https://site.net/export", names: ["logo"], chosen: false }, 4, "https://site.net/form");
  submit(w, {
    requestId: "9",
    url: "https://site.net/export",
    requestBody: { formData: { year: ["2026"], logo: [""] } },
    contentType: "multipart/form-data; boundary=----x",
  });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/export" }));
  const add = w.sent.find((m) => m.op === "add");
  const type = add.headers["Content-Type"];
  const boundary = type.split("boundary=")[1];
  assert.match(type, /^multipart\/form-data; boundary=/);
  assert.equal(
    Buffer.from(add.body, "base64").toString(),
    `--${boundary}\r\nContent-Disposition: form-data; name="year"\r\n\r\n2026\r\n--${boundary}--\r\n`,
  );
});

test("a web player's file asked for with a script's POST goes as the same POST", async () => {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({
    op: "settings",
    settings: { capture_types: [], video_auto_capture: true, video_auto_types: ["mp4"], capture_excluded_sites: [] },
  });
  const url = "https://media.net/get";
  w.chrome.webRequest.onBeforeRequest.fire({
    requestId: "x7",
    url,
    method: "POST",
    type: "xmlhttprequest",
    tabId: 3,
    requestBody: { raw: [{ bytes: new TextEncoder().encode('{"id":42}').buffer }] },
  });
  w.chrome.webRequest.onBeforeSendHeaders.fire({ requestId: "x7", url, method: "POST", requestHeaders: [{ name: "Content-Type", value: "application/json" }] });
  await w.chrome.webRequest.onHeadersReceived.fire({
    requestId: "x7",
    url,
    method: "POST",
    type: "media",
    tabId: 3,
    statusCode: 200,
    responseHeaders: [
      { name: "Content-Type", value: "video/mp4" },
      { name: "Content-Length", value: "9000000" },
    ],
  });
  await new Promise((r) => setTimeout(r, 0));
  const add = w.sent.find((m) => m.op === "add");
  same([add.method, Buffer.from(add.body, "base64").toString(), add.headers["Content-Type"]], ["POST", '{"id":42}', "application/json"]);
  // A script's POST is never taken for a link's GET to the same address.
  await w.chrome.downloads.onCreated.fire(download({ url }));
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).method, undefined);
});

test("a POST UDM cannot start goes back to the browser as a POST", async () => {
  const w = connected();
  submit(w, {
    requestId: "3",
    url: "https://site.net/export",
    requestBody: { formData: { report: ["q3"] } },
    contentType: "application/x-www-form-urlencoded",
  });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/export" }));
  w.port.onMessage.fire({ op: "ack", id: 5, url: "https://site.net/export", status: "accepted" });
  w.port.onMessage.fire({ op: "failed", id: 5, url: "https://site.net/export", error: "server returned HTTP 500" });
  const back = w.browserCalls.find((c) => c[0] === "download");
  same(
    [back[1], back[2].method, back[2].body, back[2].headers[0].value],
    ["https://site.net/export", "POST", "report=q3", "application/x-www-form-urlencoded"],
  );
});

test("the browser's fixed proxy goes along with a download", async () => {
  const add = async (value) => {
    setBrowserProxy(value);
    const w = connected();
    await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/a.zip" }));
    return w.sent.find((m) => m.op === "add").proxy;
  };
  assert.equal(await add({ mode: "fixed_servers", rules: { singleProxy: { scheme: "http", host: "p.corp", port: 3128 } } }), "http://p.corp:3128");
  assert.equal(await add({ mode: "fixed_servers", rules: { proxyForHttps: { scheme: "socks5", host: "s", port: 9050 } } }), "socks5://s:9050");
  assert.equal(await add({ mode: "fixed_servers", rules: { singleProxy: { scheme: "https", host: "p", port: 443 } } }), undefined);
  assert.equal(await add({ mode: "system" }), undefined);
  assert.equal(await add({ mode: "pac_script" }), undefined);
  setBrowserProxy({ mode: "system" });
});

test("Firefox's manual proxy goes along with a download", async () => {
  const add = async (value) => {
    setBrowserProxy(value);
    const w = connected();
    await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/a.zip" }));
    return w.sent.find((m) => m.op === "add").proxy;
  };
  assert.equal(await add({ proxyType: "manual", http: "p.corp:3128", ssl: "s.corp:8443" }), "http://s.corp:8443");
  assert.equal(await add({ proxyType: "manual", http: "p.corp:3128", ssl: "s.corp:8443", httpProxyAll: true }), "http://p.corp:3128");
  assert.equal(await add({ proxyType: "manual", http: "p.corp", ssl: "" }), "http://p.corp:80");
  assert.equal(await add({ proxyType: "manual", http: "", socks: "[::1]:9050", socksVersion: 5 }), "socks5://[::1]:9050");
  assert.equal(await add({ proxyType: "manual", socks: "s:1080", socksVersion: 4 }), "socks4://s:1080");
  assert.equal(await add({ proxyType: "manual", http: "", ssl: "", socks: "" }), undefined);
  assert.equal(await add({ proxyType: "system", http: "p.corp:3128" }), undefined);
  assert.equal(await add({ proxyType: "autoConfig", autoConfigUrl: "http://wpad/x.pac" }), undefined);
  setBrowserProxy({ mode: "system" });
});

test("a download waiting for a new link takes the next browser download", async () => {
  const w = connected();
  // Even a type the rules would leave in the browser.
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: ["zip"], capture_min_size_kb: 0, capture_excluded_sites: [] } });
  w.port.onMessage.fire({ op: "refresh_waiting", id: 9 });
  assert.equal((await w.popup({ type: "state" })).refreshWaiting, 9);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/fresh?sig=2", filename: "C:\\dl\\movie.mkv" }));
  const r = w.sent.find((m) => m.op === "refresh_link");
  same([r.id, r.download.url, r.download.filename, r.download.op], [9, "https://site.net/fresh?sig=2", "movie.mkv", undefined]);
  assert.equal(w.sent.filter((m) => m.op === "add").length, 0, "no new download");
  // Only once: the next one is an ordinary capture again.
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/b.zip" }));
  assert.equal(w.sent.filter((m) => m.op === "add").length, 1);

  w.port.onMessage.fire({ op: "refresh_waiting", id: 4 });
  await w.popup({ type: "cancelRefresh" });
  same(w.sent.at(-1), { op: "cancel_refresh" });
  assert.equal((await w.popup({ type: "state" })).refreshWaiting, null);
});
