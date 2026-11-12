//! Pairing phones (or any browser on the home network) with the daemon's
//! remote-control page, and recognizing them afterwards.
//!
//! The window shows a one-time `PairingCode` (in a QR code), which lives
//! for `PAIRING_LIFETIME` seconds and works once. A browser that sends it
//! back gets a `Credential`: its `DeviceId` and a random `DeviceSecret`,
//! which it sends with every request after. The daemon keeps only each
//! secret's `Verifier` (a digest of it) in its `Device` list, and compares
//! in constant time. `Throttle` counts wrong codes and credentials per
//! address, so guessing is refused after a few tries.
//!
//! These are the rules only: the random bytes, the digest, the clock and
//! the file the devices are kept in come in through the `PairingKeys` and
//! `PairedDevices` ports, and times as seconds since 1970.

use std::fmt;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::id::DeviceId;

/// How long a pairing code works, in seconds.
pub const PAIRING_LIFETIME: u64 = 300;
/// How many devices may be paired at once.
pub const MAX_DEVICES: usize = 20;
/// The random bytes a code or a secret is made from.
pub const RANDOM_BYTES: usize = 32;
/// The bytes of a pairing code: 128 bits.
const CODE_BYTES: usize = 16;
/// A code's characters: 128 bits at 5 bits each.
const CODE_LEN: usize = 26;
/// Crockford's base 32: no I, L, O or U, so nothing reads as another.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// A device's last-seen time is saved when it moved by this much.
pub const SEEN_EVERY: u64 = 600;
/// The longest device name kept, in characters.
pub const MAX_NAME: usize = 40;

/// Whether `a` and `b` hold the same bytes, taking as long whatever the
/// first difference is, so the time taken does not tell how much matched.
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Why pairing or a device's request was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum PairingError {
    /// The code is not the one shown, or no code is shown (or it ran out).
    #[error("That pairing code is wrong or has run out. Show a new one in UDM's Options.")]
    WrongCode,
    /// Too many wrong tries from this address; wait this many seconds.
    #[error("Too many wrong tries. Try again in {wait} seconds.")]
    TooManyTries { wait: u64 },
    /// The device is not paired (or was removed), or its secret is wrong.
    #[error("This device is not paired with UDM.")]
    NotPaired,
    #[error("{MAX_DEVICES} devices are paired already. Remove one in Options first.")]
    TooManyDevices,
    #[error("There is no paired device {0}.")]
    NoDevice(DeviceId),
    /// Remote control is off, or its page could not be served.
    #[error("Remote control is not on.")]
    NotListening,
    /// The device list or the random source failed, in its own words.
    #[error("{0}")]
    Store(String),
}

/// The one-time code a phone pairs with: 26 characters of Crockford's
/// base 32 (128 random bits). Typed, it may have dashes or spaces and any
/// case; `O` reads as 0 and `I` and `L` as 1.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PairingCode(String);

impl PairingCode {
    /// The code made from the first 16 of `random`.
    pub fn from_random(random: &[u8; RANDOM_BYTES]) -> Self {
        let mut out = String::with_capacity(CODE_LEN);
        let (mut buffer, mut bits) = (0u32, 0u32);
        for &byte in &random[..CODE_BYTES] {
            buffer = (buffer << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
            }
        }
        if bits > 0 {
            out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
        }
        PairingCode(out)
    }

    /// A code as typed or sent.
    pub fn parse(text: &str) -> Result<Self, PairingError> {
        let mut code = String::with_capacity(CODE_LEN);
        for c in text.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) || code.len() == CODE_LEN {
                return Err(PairingError::WrongCode);
            }
            code.push(c);
        }
        if code.len() == CODE_LEN {
            Ok(PairingCode(code))
        } else {
            Err(PairingError::WrongCode)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The code in groups of four, for reading out and typing.
    pub fn grouped(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        chars.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
    }

    /// Whether `other` is this code, in constant time.
    pub fn matches(&self, other: &PairingCode) -> bool {
        same(self.0.as_bytes(), other.0.as_bytes())
    }
}

/// Never the code itself, so a log line cannot hold it.
impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(..)")
    }
}

impl TryFrom<String> for PairingCode {
    type Error = PairingError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        PairingCode::parse(&text)
    }
}

impl From<PairingCode> for String {
    fn from(code: PairingCode) -> Self {
        code.0
    }
}

/// The secret a paired device sends with each request: 256 random bits.
/// It is never kept by the daemon, only its `Verifier`.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSecret([u8; RANDOM_BYTES]);

impl DeviceSecret {
    pub fn from_random(random: [u8; RANDOM_BYTES]) -> Self {
        DeviceSecret(random)
    }

    /// A secret as 64 hex digits.
    pub fn parse(hex: &str) -> Option<Self> {
        Some(DeviceSecret(decode_hex(hex)?))
    }

    pub fn bytes(&self) -> &[u8; RANDOM_BYTES] {
        &self.0
    }

    /// The secret as 64 hex digits.
    pub fn to_hex(&self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for DeviceSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceSecret(..)")
    }
}

/// What the daemon keeps to recognize a device's secret: its SHA-256
/// digest (`PairingKeys::verifier`), saved as hex.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Verifier([u8; RANDOM_BYTES]);

impl Verifier {
    pub fn new(digest: [u8; RANDOM_BYTES]) -> Self {
        Verifier(digest)
    }

    /// Whether `other` is this verifier, in constant time.
    pub fn matches(&self, other: &Verifier) -> bool {
        same(&self.0, &other.0)
    }
}

impl fmt::Debug for Verifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Verifier(..)")
    }
}

/// Why a saved verifier could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a device's verifier is not 64 hex digits")]
pub struct BadVerifier;

impl TryFrom<String> for Verifier {
    type Error = BadVerifier;

    fn try_from(hex: String) -> Result<Self, Self::Error> {
        decode_hex(&hex).map(Verifier).ok_or(BadVerifier)
    }
}

impl From<Verifier> for String {
    fn from(v: Verifier) -> Self {
        encode_hex(&v.0)
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(hex: &str) -> Option<[u8; RANDOM_BYTES]> {
    let hex = hex.as_bytes();
    if hex.len() != RANDOM_BYTES * 2 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = [0u8; RANDOM_BYTES];
    for (byte, pair) in out.iter_mut().zip(hex.chunks(2)) {
        *byte = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Some(out)
}

/// What a paired device proves itself with: its id and its secret, sent
/// as `<id>.<secret in hex>`.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub device: DeviceId,
    pub secret: DeviceSecret,
}

impl Credential {
    /// A credential as a device sends it.
    pub fn parse(text: &str) -> Option<Self> {
        let (id, secret) = text.trim().split_once('.')?;
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Credential { device: DeviceId::new(id.parse().ok()?), secret: DeviceSecret::parse(secret)? })
    }

    /// The text a device keeps and sends.
    pub fn to_text(&self) -> String {
        format!("{}.{}", self.device, self.secret.to_hex())
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Credential({}, ..)", self.device)
    }
}

/// A device's name as its browser describes it ("iPhone", "Android
/// phone"): without control characters, spaces collapsed, at most
/// `MAX_NAME` characters. It may be empty.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct DeviceName(String);

impl DeviceName {
    pub fn clean(text: &str) -> Self {
        let words: Vec<&str> =
            text.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()).collect();
        DeviceName(words.join(" ").chars().filter(|c| !c.is_control()).take(MAX_NAME).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for DeviceName {
    fn from(text: String) -> Self {
        DeviceName::clean(&text)
    }
}

impl From<DeviceName> for String {
    fn from(name: DeviceName) -> Self {
        name.0
    }
}

/// A paired device, as the daemon keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    #[serde(default)]
    pub name: DeviceName,
    /// When it paired, in seconds since 1970.
    pub paired_at: u64,
    /// When it last made a request, to within `SEEN_EVERY`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<u64>,
    pub verifier: Verifier,
}

impl Device {
    /// What the window shows of it.
    pub fn info(&self) -> DeviceInfo {
        DeviceInfo { id: self.id, name: self.name.clone(), paired_at: self.paired_at, last_seen: self.last_seen }
    }
}

/// A paired device as the window lists it: no verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    #[serde(default)]
    pub name: DeviceName,
    pub paired_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<u64>,
}

/// The code the window shows, until it is used, withdrawn or runs out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPairing {
    pub code: PairingCode,
    /// In seconds since 1970.
    pub expires_at: u64,
}

impl PendingPairing {
    pub fn expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

/// How many wrong tries a `Throttle` allows: `tries` within `per`
/// seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub tries: usize,
    pub per: u64,
}

/// Wrong pairing codes allowed from one address.
pub const PAIR_LIMIT: Limit = Limit { tries: 5, per: 60 };
/// Wrong pairing codes from every address together after which the code
/// shown is withdrawn: someone is guessing.
pub const PAIR_LIMIT_ALL: Limit = Limit { tries: 20, per: 600 };
/// Wrong credentials allowed from one address.
pub const DEVICE_LIMIT: Limit = Limit { tries: 10, per: 60 };
/// The wrong tries a throttle remembers, so a flood cannot fill memory.
const MAX_MISSES: usize = 1024;

/// Wrong tries by address and time.
#[derive(Debug, Clone, Default)]
pub struct Throttle {
    misses: Vec<(IpAddr, u64)>,
}

impl Throttle {
    /// How many seconds `peer` must wait before trying again, if it has
    /// run out of tries.
    pub fn wait(&self, limit: Limit, peer: IpAddr, now: u64) -> Option<u64> {
        let recent: Vec<u64> =
            self.misses.iter().filter(|(p, at)| *p == peer && at + limit.per > now).map(|(_, at)| *at).collect();
        if recent.len() < limit.tries {
            return None;
        }
        // Once enough of the oldest have aged out, one try is free again.
        let freed = recent[recent.len() - limit.tries];
        Some((freed + limit.per).saturating_sub(now).max(1))
    }

    /// Note a wrong try from `peer`.
    pub fn missed(&mut self, peer: IpAddr, now: u64) {
        if self.misses.len() >= MAX_MISSES {
            self.misses.remove(0);
        }
        self.misses.push((peer, now));
    }

    /// Wrong tries from anyone within the last `per` seconds.
    pub fn recent(&self, per: u64, now: u64) -> usize {
        self.misses.iter().filter(|(_, at)| at + per > now).count()
    }

    /// Forget tries older than `per` seconds.
    pub fn forget_before(&mut self, per: u64, now: u64) {
        self.misses.retain(|(_, at)| at + per > now);
    }
}

/// The paired devices, the code being shown and the wrong tries: what the
/// daemon knows about pairing while it runs.
#[derive(Debug, Clone, Default)]
pub struct Pairings {
    devices: Vec<Device>,
    pending: Option<PendingPairing>,
    pair_misses: Throttle,
    device_misses: Throttle,
}

impl Pairings {
    /// The devices paired before.
    pub fn new(devices: Vec<Device>) -> Self {
        Pairings { devices, ..Pairings::default() }
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    /// The code shown, unless it ran out.
    pub fn pending(&self, now: u64) -> Option<&PendingPairing> {
        self.pending.as_ref().filter(|p| !p.expired(now))
    }

    /// Show `code` from `now` on, in place of any code shown before.
    pub fn start(&mut self, code: PairingCode, now: u64) -> &PendingPairing {
        self.pending.insert(PendingPairing { code, expires_at: now + PAIRING_LIFETIME })
    }

    /// Withdraw the code shown.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Whether another device may pair.
    pub fn has_room(&self) -> Result<(), PairingError> {
        if self.devices.len() >= MAX_DEVICES {
            Err(PairingError::TooManyDevices)
        } else {
            Ok(())
        }
    }

    /// Use up the code shown, if `text` is it and `peer` has tries left.
    /// A wrong code counts against `peer`; after too many from everyone,
    /// the code is withdrawn.
    pub fn redeem(&mut self, text: &str, peer: IpAddr, now: u64) -> Result<(), PairingError> {
        self.pair_misses.forget_before(PAIR_LIMIT_ALL.per.max(PAIR_LIMIT.per), now);
        if let Some(wait) = self.pair_misses.wait(PAIR_LIMIT, peer, now) {
            return Err(PairingError::TooManyTries { wait });
        }
        let right = match (PairingCode::parse(text), self.pending(now)) {
            (Ok(code), Some(pending)) => pending.code.matches(&code),
            _ => false,
        };
        if right {
            self.pending = None;
            return Ok(());
        }
        self.pair_misses.missed(peer, now);
        if self.pair_misses.recent(PAIR_LIMIT_ALL.per, now) >= PAIR_LIMIT_ALL.tries {
            self.pending = None;
        }
        Err(PairingError::WrongCode)
    }

    /// Add a device that redeemed the code, recognized by `verifier`.
    pub fn add(&mut self, name: DeviceName, verifier: Verifier, now: u64) -> Result<DeviceId, PairingError> {
        self.has_room()?;
        let id = self.devices.iter().map(|d| d.id).max().unwrap_or_default().next();
        self.devices.push(Device { id, name, paired_at: now, last_seen: Some(now), verifier });
        Ok(id)
    }

    /// The device `text` names, if its secret's verifier (`verifier` makes
    /// it) is the one kept and `peer` has tries left. Returns whether its
    /// last-seen time moved enough to be saved.
    pub fn check(
        &mut self,
        text: &str,
        peer: IpAddr,
        now: u64,
        verifier: impl Fn(&DeviceSecret) -> Verifier,
    ) -> Result<(DeviceId, bool), PairingError> {
        self.device_misses.forget_before(DEVICE_LIMIT.per, now);
        if let Some(wait) = self.device_misses.wait(DEVICE_LIMIT, peer, now) {
            return Err(PairingError::TooManyTries { wait });
        }
        let found = Credential::parse(text).and_then(|c| {
            let wanted = verifier(&c.secret);
            // The digest is worked out whether or not the id is known, and
            // compared in constant time.
            self.devices.iter_mut().find(|d| d.id == c.device).filter(|d| d.verifier.matches(&wanted))
        });
        match found {
            Some(device) => {
                let save = device.last_seen.is_none_or(|seen| now >= seen + SEEN_EVERY);
                if save {
                    device.last_seen = Some(now);
                }
                Ok((device.id, save))
            }
            None => {
                self.device_misses.missed(peer, now);
                Err(PairingError::NotPaired)
            }
        }
    }

    /// Unpair device `id`: its credential stops working at once.
    pub fn remove(&mut self, id: DeviceId) -> Result<Device, PairingError> {
        let at = self.devices.iter().position(|d| d.id == id).ok_or(PairingError::NoDevice(id))?;
        Ok(self.devices.remove(at))
    }
}

/// Why the remote-control page is not being served although it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteFault {
    /// Another program listens on this address and port.
    PortInUse { address: String },
    /// This computer has no such address.
    AddressUnavailable { address: String },
    /// The system does not let UDM listen there (a port below 1024).
    NoPermission { address: String },
    /// The page's certificate could not be made or read, and why.
    Certificate(String),
    /// Something else, in the system's words.
    Other(String),
}

/// Whether and where the page is served, as the listener reports it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ListenerState {
    /// The addresses and ports listened on.
    #[serde(default)]
    pub listening: Vec<String>,
    /// The page's address for a phone on the network, when listening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Why it is on but not listening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<RemoteFault>,
    /// The SHA-256 fingerprint of the page's certificate, as browsers
    /// show it (`AB:CD:...`), once there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// A pairing code being shown, with what the window puts in its QR code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingOffer {
    /// The page's address with the code, for the QR code.
    pub link: String,
    pub code: PairingCode,
    /// In seconds since 1970.
    pub expires_at: u64,
}

/// Remote control as the window shows it in Options.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RemoteState {
    #[serde(flatten)]
    pub listener: ListenerState,
    #[serde(default)]
    pub devices: Vec<DeviceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<PairingOffer>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    const NOW: u64 = 1_700_000_000;

    fn peer(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, n))
    }

    fn random(n: u8) -> [u8; RANDOM_BYTES] {
        let mut r = [0u8; RANDOM_BYTES];
        for (i, b) in r.iter_mut().enumerate() {
            *b = n.wrapping_mul(31).wrapping_add(i as u8);
        }
        r
    }

    /// A stand-in digest: not SHA-256, but one secret, one verifier.
    fn digest(s: &DeviceSecret) -> Verifier {
        let mut d = *s.bytes();
        d.reverse();
        Verifier::new(d)
    }

    #[test]
    fn a_code_is_26_characters_of_128_bits_and_reads_back_however_typed() {
        let code = PairingCode::from_random(&random(1));
        assert_eq!(code.as_str().len(), 26);
        assert!(code.as_str().bytes().all(|b| ALPHABET.contains(&b)));
        assert_ne!(code, PairingCode::from_random(&random(2)));
        let typed = code.grouped().to_lowercase().replace('0', "o").replace('1', "l");
        assert_eq!(PairingCode::parse(&typed), Ok(code.clone()));
        assert_eq!(PairingCode::parse(&format!(" {} ", code.grouped())), Ok(code.clone()));
        assert_eq!(code.grouped().matches('-').count(), 6);
        let all_ones = PairingCode::from_random(&[0xff; RANDOM_BYTES]);
        assert_eq!(all_ones.as_str(), "ZZZZZZZZZZZZZZZZZZZZZZZZZW");
    }

    #[test]
    fn a_code_of_the_wrong_length_or_letters_is_refused() {
        for bad in
            ["", "ABC", "U234567890123456789012345", "0123456789012345678901234567", "ÄBCDEFGHJKMNPQRSTVWXYZ0123"]
        {
            assert_eq!(PairingCode::parse(bad), Err(PairingError::WrongCode), "{bad}");
        }
    }

    #[test]
    fn nothing_secret_shows_in_debug_output() {
        let code = PairingCode::from_random(&random(1));
        let secret = DeviceSecret::from_random(random(2));
        let credential = Credential { device: DeviceId::new(3), secret: secret.clone() };
        let shown = format!("{code:?} {secret:?} {credential:?} {:?}", digest(&secret));
        assert!(!shown.contains(code.as_str()));
        assert!(!shown.contains(&secret.to_hex()));
        assert!(!shown.contains(&String::from(digest(&secret))));
    }

    #[test]
    fn a_credential_is_the_id_and_the_secret_in_hex() {
        let c = Credential { device: DeviceId::new(12), secret: DeviceSecret::from_random(random(4)) };
        let text = c.to_text();
        assert!(text.starts_with("12."));
        assert_eq!(text.len(), 3 + 64);
        assert_eq!(Credential::parse(&text), Some(c));
        for bad in ["", "12", "12.", ".abc", "x.00", "-1.00", &format!("12.{}", "g".repeat(64))] {
            assert_eq!(Credential::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn verifiers_are_hex_in_json_and_damaged_ones_are_refused() {
        let v = Verifier::new([0xab; RANDOM_BYTES]);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
        assert_eq!(serde_json::from_str::<Verifier>(&json).unwrap(), v);
        assert!(serde_json::from_str::<Verifier>("\"abc\"").is_err());
    }

    #[test]
    fn names_lose_control_characters_and_their_length() {
        assert_eq!(DeviceName::clean("  iPhone \n\t Safari\u{7}  ").as_str(), "iPhone Safari");
        assert_eq!(DeviceName::clean(&"x".repeat(100)).as_str().len(), MAX_NAME);
        assert!(DeviceName::clean("\u{0}\r\n").is_empty());
    }

    #[test]
    fn a_code_pairs_once_within_its_lifetime() {
        let mut p = Pairings::default();
        let code = PairingCode::from_random(&random(1));
        p.start(code.clone(), NOW);
        assert!(p.pending(NOW + PAIRING_LIFETIME - 1).is_some());
        assert_eq!(p.redeem(code.as_str(), peer(2), NOW + 10), Ok(()));
        assert_eq!(p.redeem(code.as_str(), peer(2), NOW + 11), Err(PairingError::WrongCode), "used up");

        p.start(code.clone(), NOW);
        assert!(p.pending(NOW + PAIRING_LIFETIME).is_none());
        assert_eq!(p.redeem(code.as_str(), peer(3), NOW + PAIRING_LIFETIME), Err(PairingError::WrongCode));

        p.start(code.clone(), NOW);
        p.cancel();
        assert_eq!(p.redeem(code.as_str(), peer(4), NOW + 1), Err(PairingError::WrongCode), "withdrawn");
    }

    #[test]
    fn a_new_code_replaces_the_old_one() {
        let mut p = Pairings::default();
        let (old, new) = (PairingCode::from_random(&random(1)), PairingCode::from_random(&random(2)));
        p.start(old.clone(), NOW);
        p.start(new.clone(), NOW);
        assert_eq!(p.redeem(old.as_str(), peer(2), NOW), Err(PairingError::WrongCode));
        assert_eq!(p.redeem(new.as_str(), peer(2), NOW), Ok(()));
    }

    #[test]
    fn wrong_codes_lock_out_their_address_for_a_while() {
        let mut p = Pairings::default();
        let code = PairingCode::from_random(&random(1));
        p.start(code.clone(), NOW);
        for i in 0..PAIR_LIMIT.tries as u64 {
            assert_eq!(p.redeem("0000-0000", peer(9), NOW + i), Err(PairingError::WrongCode));
        }
        // Even the right code is refused from there now.
        assert_eq!(p.redeem(code.as_str(), peer(9), NOW + 5), Err(PairingError::TooManyTries { wait: 55 }));
        // Another address still pairs.
        assert_eq!(p.redeem(code.as_str(), peer(10), NOW + 5), Ok(()));
        p.start(code.clone(), NOW + 61);
        assert_eq!(p.redeem(code.as_str(), peer(9), NOW + 61), Ok(()), "after a minute");
    }

    #[test]
    fn many_wrong_codes_from_everywhere_withdraw_the_code() {
        let mut p = Pairings::default();
        let code = PairingCode::from_random(&random(1));
        p.start(code.clone(), NOW);
        for n in 0..PAIR_LIMIT_ALL.tries as u8 {
            assert_eq!(p.redeem("bad", peer(n), NOW), Err(PairingError::WrongCode));
        }
        assert!(p.pending(NOW).is_none());
        assert_eq!(p.redeem(code.as_str(), peer(200), NOW), Err(PairingError::WrongCode));
    }

    #[test]
    fn a_paired_device_is_recognized_until_it_is_removed() {
        let mut p = Pairings::default();
        let secret = DeviceSecret::from_random(random(5));
        let id = p.add(DeviceName::clean("Phone"), digest(&secret), NOW).unwrap();
        assert_eq!(id, DeviceId::new(1));
        let text = Credential { device: id, secret: secret.clone() }.to_text();
        assert_eq!(p.check(&text, peer(2), NOW + 1, digest), Ok((id, false)));
        assert_eq!(p.check(&text, peer(2), NOW + SEEN_EVERY, digest), Ok((id, true)), "seen again, saved");
        let other = Credential { device: id, secret: DeviceSecret::from_random(random(6)) }.to_text();
        assert_eq!(p.check(&other, peer(2), NOW + 2, digest), Err(PairingError::NotPaired));
        let unknown = Credential { device: DeviceId::new(7), secret }.to_text();
        assert_eq!(p.check(&unknown, peer(2), NOW + 2, digest), Err(PairingError::NotPaired));
        assert_eq!(p.remove(id).map(|d| d.id), Ok(id));
        assert_eq!(p.check(&text, peer(3), NOW + 3, digest), Err(PairingError::NotPaired));
        assert_eq!(p.remove(id), Err(PairingError::NoDevice(id)));
    }

    #[test]
    fn wrong_credentials_lock_out_their_address() {
        let mut p = Pairings::default();
        let secret = DeviceSecret::from_random(random(5));
        let id = p.add(DeviceName::default(), digest(&secret), NOW).unwrap();
        for i in 0..DEVICE_LIMIT.tries as u64 {
            assert_eq!(p.check("1.00", peer(8), NOW + i, digest), Err(PairingError::NotPaired));
        }
        let text = Credential { device: id, secret }.to_text();
        assert!(matches!(p.check(&text, peer(8), NOW + 10, digest), Err(PairingError::TooManyTries { .. })));
        assert!(p.check(&text, peer(7), NOW + 10, digest).is_ok());
        assert!(p.check(&text, peer(8), NOW + 70, digest).is_ok());
    }

    #[test]
    fn devices_get_rising_ids_up_to_the_limit() {
        let mut p = Pairings::default();
        for n in 0..MAX_DEVICES {
            let id = p.add(DeviceName::default(), Verifier::new([n as u8; RANDOM_BYTES]), NOW).unwrap();
            assert_eq!(id.get(), n as u64 + 1);
        }
        assert_eq!(p.has_room(), Err(PairingError::TooManyDevices));
        assert_eq!(p.add(DeviceName::default(), Verifier::new([0; 32]), NOW), Err(PairingError::TooManyDevices));
        p.remove(DeviceId::new(3)).unwrap();
        let id = p.add(DeviceName::default(), Verifier::new([0; 32]), NOW).unwrap();
        assert_eq!(id.get(), MAX_DEVICES as u64 + 1, "a removed device's id is not given again");
    }

    #[test]
    fn a_throttle_remembers_a_bounded_number_of_tries() {
        let mut t = Throttle::default();
        for i in 0..(MAX_MISSES as u64 + 10) {
            t.missed(peer((i % 200) as u8), NOW);
        }
        assert_eq!(t.recent(60, NOW), MAX_MISSES);
        t.forget_before(60, NOW + 60);
        assert_eq!(t.recent(600, NOW + 60), 0);
    }

    #[test]
    fn devices_keep_their_shape_in_json() {
        let d = Device {
            id: DeviceId::new(2),
            name: DeviceName::clean("Pixel"),
            paired_at: NOW,
            last_seen: None,
            verifier: Verifier::new([1; RANDOM_BYTES]),
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json, serde_json::json!({"id": 2, "name": "Pixel", "paired_at": NOW, "verifier": "01".repeat(32)}));
        assert_eq!(serde_json::from_value::<Device>(json).unwrap(), d);
        let info = serde_json::to_value(d.info()).unwrap();
        assert!(info.get("verifier").is_none());
    }
}
