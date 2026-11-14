// Signs a release's files, so Update now installs only what the release
// workflow built (#267). UDM pins the public key in
// crates/infrastructure/src/release-key.pub and refuses a file that the
// signed list does not name with the same SHA-256.
//
// Make the key pair once, keep the secret file out of the repository, and
// put its one line in the repository's UDM_RELEASE_KEY Actions secret:
//
//   node installer/sign-checksums.js keygen <secret-file> <public-file>
//
// The release workflow then lists every file in dist/ in dist/SHA256SUMS
// (sha256sum's format) and signs the list into dist/SHA256SUMS.minisig,
// with the secret line in the UDM_RELEASE_KEY environment variable:
//
//   node installer/sign-checksums.js sign dist
//
// and checks the result against the pinned key before publishing:
//
//   node installer/sign-checksums.js verify dist crates/infrastructure/src/release-key.pub
//
// The public key and the signature are in minisign's formats (Ed25519, the
// list prehashed with BLAKE2b-512), so anyone can check a download with
// `minisign -Vm SHA256SUMS -p release-key.pub`. The secret file is this
// script's own: the key's id and its 32-byte seed, in base64. Node's
// crypto does all of it; nothing is installed.

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const LIST = "SHA256SUMS";
const SIGNATURE = "SHA256SUMS.minisig";
// What Node's DER encoding puts before an Ed25519 key's 32 bytes.
const PKCS8_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");
const SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

// The secret line as the key's id (8 bytes) and its private key.
function readSecret(line) {
  const raw = Buffer.from(line.trim(), "base64");
  if (raw.length !== 40) {
    throw new Error("the secret key is not 40 bytes of base64; make it with `keygen`");
  }
  const id = raw.subarray(0, 8);
  const key = crypto.createPrivateKey({ key: Buffer.concat([PKCS8_PREFIX, raw.subarray(8)]), format: "der", type: "pkcs8" });
  return { id, key };
}

// The 32 bytes of `key`'s public half.
function rawPublic(key) {
  const der = crypto.createPublicKey(key).export({ format: "der", type: "spki" });
  return der.subarray(der.length - 32);
}

// minisign's public key file for `id` and the 32-byte public key.
function publicFile(id, pk) {
  const hex = Buffer.from(id).reverse().toString("hex").toUpperCase();
  const body = Buffer.concat([Buffer.from("Ed"), id, pk]).toString("base64");
  return `untrusted comment: minisign public key ${hex}\n${body}\n`;
}

// The id and public key in a minisign public key file's text.
function readPublic(text) {
  const line = text.split(/\r?\n/).find((l) => l && !l.startsWith("untrusted comment:"));
  const raw = Buffer.from(line || "", "base64");
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== "Ed") {
    throw new Error("not a minisign public key");
  }
  const der = Buffer.concat([SPKI_PREFIX, raw.subarray(10)]);
  return { id: raw.subarray(2, 10), key: crypto.createPublicKey({ key: der, format: "der", type: "spki" }) };
}

function keygen(secretFile, publicFile_) {
  const id = crypto.randomBytes(8);
  const { privateKey } = crypto.generateKeyPairSync("ed25519");
  const der = privateKey.export({ format: "der", type: "pkcs8" });
  const seed = der.subarray(der.length - 32);
  fs.writeFileSync(secretFile, Buffer.concat([id, seed]).toString("base64") + "\n", { mode: 0o600, flag: "wx" });
  fs.writeFileSync(publicFile_, publicFile(id, rawPublic(privateKey)));
}

// `sha256sum`'s listing of every file in `dir` but the list and signature.
function listing(dir) {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && e.name !== LIST && e.name !== SIGNATURE)
    .map((e) => e.name)
    .sort()
    .map((name) => {
      const hash = crypto.createHash("sha256").update(fs.readFileSync(path.join(dir, name))).digest("hex");
      return `${hash}  ${name}\n`;
    })
    .join("");
}

// The text of a minisign signature of `data` by `secret`.
function signature(data, secret, trusted) {
  const hashed = crypto.createHash("blake2b512").update(data).digest();
  const sig = crypto.sign(null, hashed, secret.key);
  const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted)]), secret.key);
  return [
    "untrusted comment: signature from UDM's release key",
    Buffer.concat([Buffer.from("ED"), secret.id, sig]).toString("base64"),
    `trusted comment: ${trusted}`,
    global.toString("base64"),
    "",
  ].join("\n");
}

// Whether `sigText` is a good signature of `data` by `pub`.
function verified(data, sigText, pub) {
  const lines = sigText.split(/\r?\n/);
  const raw = Buffer.from(lines[1] || "", "base64");
  const trusted = (lines[2] || "").replace(/^trusted comment: /, "");
  if (raw.length !== 74 || raw.subarray(0, 2).toString() !== "ED" || !raw.subarray(2, 10).equals(pub.id)) {
    return false;
  }
  const sig = raw.subarray(10);
  const hashed = crypto.createHash("blake2b512").update(data).digest();
  return (
    crypto.verify(null, hashed, pub.key, sig) &&
    crypto.verify(null, Buffer.concat([sig, Buffer.from(trusted)]), pub.key, Buffer.from(lines[3] || "", "base64"))
  );
}

function sign(dir) {
  const line = process.env.UDM_RELEASE_KEY;
  if (!line) {
    throw new Error("UDM_RELEASE_KEY is not set");
  }
  const secret = readSecret(line);
  const list = listing(dir);
  if (!list) {
    throw new Error(`${dir} holds no files to sign`);
  }
  fs.writeFileSync(path.join(dir, LIST), list);
  const trusted = `timestamp:${Math.floor(Date.now() / 1000)}\tfile:${LIST}\thashed`;
  fs.writeFileSync(path.join(dir, SIGNATURE), signature(Buffer.from(list), secret, trusted));
}

function verify(dir, publicKeyFile) {
  const pub = readPublic(fs.readFileSync(publicKeyFile, "utf8"));
  const list = fs.readFileSync(path.join(dir, LIST));
  if (!verified(list, fs.readFileSync(path.join(dir, SIGNATURE), "utf8"), pub)) {
    throw new Error(`${SIGNATURE} is not a signature of ${LIST} by ${publicKeyFile}`);
  }
  if (list.toString() !== listing(dir)) {
    throw new Error(`${LIST} does not list the files in ${dir} as they are`);
  }
}

module.exports = { keygen, listing, publicFile, readPublic, readSecret, signature, verified, sign, verify };

if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  try {
    if (command === "keygen" && args.length === 2) {
      keygen(args[0], args[1]);
    } else if (command === "sign" && args.length === 1) {
      sign(args[0]);
    } else if (command === "verify" && args.length === 2) {
      verify(args[0], args[1]);
    } else {
      process.stderr.write("usage: sign-checksums.js keygen <secret> <public> | sign <dir> | verify <dir> <public>\n");
      process.exit(2);
    }
  } catch (e) {
    process.stderr.write(`sign-checksums: ${e.message}\n`);
    process.exit(1);
  }
}
