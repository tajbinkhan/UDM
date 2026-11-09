//! NTLM logins for servers that offer only Windows authentication (intranet
//! sites, SharePoint, some corporate file servers), as IDM answers them.
//!
//! NTLM is a three-step handshake on one connection: the client sends a
//! negotiate message, the server answers 401 with a challenge, and the
//! client answers that with an authenticate message proving it knows the
//! password. Only NTLMv2 responses are sent (MS-NLMP 3.3.2); the password
//! itself never goes to the server. No session key is negotiated, since
//! HTTP needs no signing or sealing.

use md4::{Digest as _, Md4};
use md5::Md5;

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";

const NEGOTIATE_UNICODE: u32 = 0x0000_0001;
const REQUEST_TARGET: u32 = 0x0000_0004;
const NEGOTIATE_NTLM: u32 = 0x0000_0200;
const ALWAYS_SIGN: u32 = 0x0000_8000;
const EXTENDED_SESSION_SECURITY: u32 = 0x0008_0000;
const TARGET_INFO: u32 = 0x0080_0000;
const NEGOTIATE_128: u32 = 0x2000_0000;
const NEGOTIATE_56: u32 = 0x8000_0000;

const FLAGS: u32 = NEGOTIATE_UNICODE
    | REQUEST_TARGET
    | NEGOTIATE_NTLM
    | ALWAYS_SIGN
    | EXTENDED_SESSION_SECURITY
    | TARGET_INFO
    | NEGOTIATE_128
    | NEGOTIATE_56;

/// `MsvAvTimestamp` in the challenge's target info.
const AV_TIMESTAMP: u16 = 7;

/// The first message: what the client supports.
pub fn negotiate() -> Vec<u8> {
    let mut m = Vec::with_capacity(32);
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&1u32.to_le_bytes());
    m.extend_from_slice(&FLAGS.to_le_bytes());
    // Empty domain and workstation fields.
    m.extend_from_slice(&[0; 16]);
    m
}

/// What the server's challenge message holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub flags: u32,
    pub server_challenge: [u8; 8],
    /// The AV pairs the response must echo.
    pub target_info: Vec<u8>,
}

/// Read a challenge (type 2) message.
pub fn parse_challenge(m: &[u8]) -> Option<Challenge> {
    if m.len() < 32 || &m[..8] != SIGNATURE || u32::from_le_bytes(m[8..12].try_into().ok()?) != 2 {
        return None;
    }
    let flags = u32::from_le_bytes(m[20..24].try_into().ok()?);
    let server_challenge = m[24..32].try_into().ok()?;
    let target_info = if m.len() >= 48 {
        let len = u16::from_le_bytes(m[40..42].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(m[44..48].try_into().ok()?) as usize;
        m.get(offset..offset.checked_add(len)?)?.to_vec()
    } else {
        Vec::new()
    };
    Some(Challenge { flags, server_challenge, target_info })
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hmac_md5(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..16].copy_from_slice(&Md5::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Md5::new();
    inner.update(block.map(|b| b ^ 0x36));
    for p in parts {
        inner.update(p);
    }
    let mut outer = Md5::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// NTOWFv2: the key every NTLMv2 response is made with.
fn ntowf_v2(user: &str, domain: &str, password: &str) -> [u8; 16] {
    let nt_hash = Md4::digest(utf16(password));
    hmac_md5(&nt_hash, &[&utf16(&user.to_uppercase()), &utf16(domain)])
}

/// The user and domain a login names: `DOMAIN\user` splits; anything else
/// (a plain name, or `user@domain` as Windows takes it) is the user name
/// with no domain.
pub fn split_user(login: &str) -> (&str, &str) {
    match login.split_once('\\') {
        Some((domain, user)) => (user, domain),
        None => (login, ""),
    }
}

/// The value of an AV pair in `target_info`.
fn av_pair(target_info: &[u8], id: u16) -> Option<&[u8]> {
    let mut rest = target_info;
    while rest.len() >= 4 {
        let kind = u16::from_le_bytes([rest[0], rest[1]]);
        let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
        if kind == 0 {
            return None;
        }
        let value = rest.get(4..4 + len)?;
        if kind == id {
            return Some(value);
        }
        rest = &rest[4 + len..];
    }
    None
}

/// Now as a Windows FILETIME: 100 ns units since 1601.
fn file_time_now() -> u64 {
    let since_1970 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (since_1970.as_nanos() / 100) as u64 + 116_444_736_000_000_000
}

/// The NTLMv2 response to `challenge` and the LMv2 response beside it.
/// `client_challenge` is 8 random bytes; `time` the FILETIME to put in the
/// response when the server sent none.
fn responses(
    challenge: &Challenge,
    user: &str,
    domain: &str,
    password: &str,
    client_challenge: [u8; 8],
    time: u64,
) -> (Vec<u8>, Vec<u8>) {
    let key = ntowf_v2(user, domain, password);
    let server_time = av_pair(&challenge.target_info, AV_TIMESTAMP).and_then(|t| <[u8; 8]>::try_from(t).ok());
    let mut temp = vec![1, 1, 0, 0, 0, 0, 0, 0];
    temp.extend_from_slice(&server_time.unwrap_or(time.to_le_bytes()));
    temp.extend_from_slice(&client_challenge);
    temp.extend_from_slice(&[0; 4]);
    temp.extend_from_slice(&challenge.target_info);
    temp.extend_from_slice(&[0; 4]);
    let proof = hmac_md5(&key, &[&challenge.server_challenge, &temp]);
    let mut nt = proof.to_vec();
    nt.extend_from_slice(&temp);
    // With the server's own timestamp, the LM response is left empty
    // (zeros), as MS-NLMP says.
    let lm = if server_time.is_some() {
        vec![0; 24]
    } else {
        let mut lm = hmac_md5(&key, &[&challenge.server_challenge, &client_challenge]).to_vec();
        lm.extend_from_slice(&client_challenge);
        lm
    };
    (nt, lm)
}

/// The third message: proof that `login` (`user` or `DOMAIN\user`) knows
/// `password`, for `challenge`. `workstation` is this computer's name.
pub fn authenticate(challenge: &Challenge, login: &str, password: &str, workstation: &str) -> Vec<u8> {
    let mut client_challenge = [0u8; 8];
    let _ = getrandom::fill(&mut client_challenge);
    authenticate_with(challenge, login, password, workstation, client_challenge, file_time_now())
}

fn authenticate_with(
    challenge: &Challenge,
    login: &str,
    password: &str,
    workstation: &str,
    client_challenge: [u8; 8],
    time: u64,
) -> Vec<u8> {
    let (user, domain) = split_user(login);
    let (nt, lm) = responses(challenge, user, domain, password, client_challenge, time);
    let fields: [Vec<u8>; 6] = [lm, nt, utf16(domain), utf16(user), utf16(workstation), Vec::new()];
    let flags = FLAGS & (challenge.flags | NEGOTIATE_UNICODE);
    let header = 64;
    let mut m = Vec::with_capacity(header + fields.iter().map(Vec::len).sum::<usize>());
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&3u32.to_le_bytes());
    let mut offset = header;
    for f in &fields {
        let len = f.len() as u16;
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&len.to_le_bytes());
        m.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += f.len();
    }
    m.extend_from_slice(&flags.to_le_bytes());
    for f in &fields {
        m.extend_from_slice(f);
    }
    m
}

/// This computer's name, for the workstation field.
pub fn workstation() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Pullbay".into())
}

/// A challenge message for `c`, as a server sends it. For tests.
#[cfg(test)]
pub(crate) fn challenge_message(c: &Challenge) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(SIGNATURE);
    m.extend_from_slice(&2u32.to_le_bytes());
    m.extend_from_slice(&[0; 8]);
    m.extend_from_slice(&c.flags.to_le_bytes());
    m.extend_from_slice(&c.server_challenge);
    m.extend_from_slice(&[0; 8]);
    m.extend_from_slice(&(c.target_info.len() as u16).to_le_bytes());
    m.extend_from_slice(&(c.target_info.len() as u16).to_le_bytes());
    m.extend_from_slice(&48u32.to_le_bytes());
    m.extend_from_slice(&c.target_info);
    m
}

/// Whether `msg`, an authenticate message answering `c`, proves `login`
/// knows `password`, as a server checks it. For tests.
#[cfg(test)]
pub(crate) fn check_authenticate(c: &Challenge, msg: &[u8], login: &str, password: &str) -> bool {
    let field = |at: usize| -> Option<&[u8]> {
        let len = u16::from_le_bytes(msg.get(at..at + 2)?.try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(msg.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        msg.get(offset..offset + len)
    };
    let (user, domain) = split_user(login);
    let (Some(nt), Some(sent_domain), Some(sent_user)) = (field(20), field(28), field(36)) else {
        return false;
    };
    if msg.get(..12) != Some(b"NTLMSSP\0\x03\0\0\0".as_slice())
        || sent_user != utf16(user).as_slice()
        || sent_domain != utf16(domain).as_slice()
        || nt.len() < 16
    {
        return false;
    }
    hmac_md5(&ntowf_v2(user, domain, password), &[&c.server_challenge, &nt[16..]]) == nt[..16]
}

/// A challenge with target info and a server timestamp. For tests.
#[cfg(test)]
pub(crate) fn test_challenge() -> Challenge {
    let mut info = Vec::new();
    let name = utf16("TESTSERVER");
    info.extend_from_slice(&1u16.to_le_bytes());
    info.extend_from_slice(&(name.len() as u16).to_le_bytes());
    info.extend_from_slice(&name);
    info.extend_from_slice(&AV_TIMESTAMP.to_le_bytes());
    info.extend_from_slice(&8u16.to_le_bytes());
    info.extend_from_slice(&file_time_now().to_le_bytes());
    info.extend_from_slice(&[0; 4]);
    Challenge { flags: FLAGS, server_challenge: *b"\x11\x22\x33\x44\x55\x66\x77\x88", target_info: info }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// MS-NLMP 4.2.4: user "User", domain "Domain", password "Password",
    /// server challenge 0123456789abcdef, client challenge aa..aa, time 0.
    fn spec_challenge() -> Challenge {
        let mut info = Vec::new();
        for (id, value) in [(2u16, "Domain"), (1, "Server")] {
            let v = utf16(value);
            info.extend_from_slice(&id.to_le_bytes());
            info.extend_from_slice(&(v.len() as u16).to_le_bytes());
            info.extend_from_slice(&v);
        }
        info.extend_from_slice(&[0; 4]);
        Challenge {
            flags: FLAGS,
            server_challenge: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
            target_info: info,
        }
    }

    #[test]
    fn ntlmv2_matches_the_specification() {
        assert_eq!(hex(&Md4::digest(utf16("Password"))), "a4f49c406510bdcab6824ee7c30fd852");
        assert_eq!(hex(&ntowf_v2("User", "Domain", "Password")), "0c868a403bfd7a93a3001ef22ef02e3f");
        let (nt, lm) = responses(&spec_challenge(), "User", "Domain", "Password", [0xaa; 8], 0);
        assert_eq!(hex(&nt[..16]), "68cd0ab851e51c96aabc927bebef6a1c");
        assert_eq!(hex(&lm), "86c35097ac9cec102554764a57cccc19aaaaaaaaaaaaaaaa");
    }

    #[test]
    fn hmac_md5_matches_rfc_2104() {
        // RFC 2104's second test case.
        assert_eq!(
            hex(&hmac_md5(b"Jefe", &[b"what do ya want ", b"for nothing?"])),
            "750c783e6ab0b503eaa86e310a5db738"
        );
    }

    #[test]
    fn a_challenge_is_read_and_answered() {
        let info = spec_challenge().target_info;
        let mut m = Vec::new();
        m.extend_from_slice(SIGNATURE);
        m.extend_from_slice(&2u32.to_le_bytes());
        m.extend_from_slice(&[0; 8]);
        m.extend_from_slice(&FLAGS.to_le_bytes());
        m.extend_from_slice(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        m.extend_from_slice(&[0; 8]);
        m.extend_from_slice(&(info.len() as u16).to_le_bytes());
        m.extend_from_slice(&(info.len() as u16).to_le_bytes());
        m.extend_from_slice(&48u32.to_le_bytes());
        m.extend_from_slice(&info);
        let c = parse_challenge(&m).unwrap();
        assert_eq!(c, spec_challenge());
        assert!(parse_challenge(b"NTLMSSP\0\x01\0\0\0").is_none());

        let msg = authenticate_with(&c, "Domain\\User", "Password", "COMPUTER", [0xaa; 8], 0);
        assert_eq!(&msg[..12], b"NTLMSSP\0\x03\0\0\0");
        assert!(check_authenticate(&c, &msg, "Domain\\User", "Password"));
        assert!(!check_authenticate(&c, &msg, "Domain\\User", "password"));
        assert_eq!(parse_challenge(&challenge_message(&c)), Some(c.clone()));
        // The NT response field points at the proof computed above.
        let len = u16::from_le_bytes([msg[20], msg[21]]) as usize;
        let at = u32::from_le_bytes(msg[24..28].try_into().unwrap()) as usize;
        assert_eq!(hex(&msg[at..at + 16]), "68cd0ab851e51c96aabc927bebef6a1c");
        assert_eq!(len, 16 + 28 + c.target_info.len() + 4);
        // The user name, as UTF-16.
        let at = u32::from_le_bytes(msg[40..44].try_into().unwrap()) as usize;
        assert_eq!(&msg[at..at + 8], &utf16("User")[..]);
    }

    #[test]
    fn logins_split_into_user_and_domain() {
        assert_eq!(split_user("CORP\\ann"), ("ann", "CORP"));
        assert_eq!(split_user("ann@corp.example"), ("ann@corp.example", ""));
        assert_eq!(split_user("ann"), ("ann", ""));
    }
}
