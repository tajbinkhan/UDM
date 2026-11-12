//! Pairing phones with the remote-control page, and running what a paired
//! phone asks (see `udm_domain::pairing` for the rules).
//!
//! The use cases work on the daemon's `Pairings`: `StartPairing` shows a
//! code, `PairDevice` exchanges it for a device's credential,
//! `CheckDevice` recognizes a credential, and `RemoveDevice` unpairs.
//! `PairingDesk` holds the state they share, for the window's commands
//! and the page alike. `Phone` is the page's side (the `PhoneGate` port the
//! HTTPS server in `udm-infrastructure` drives): it checks each
//! request's credential and runs it through the daemon's own commands on a
//! `LocalClient`, with the `remote` use cases `udm-cli` uses.

mod check_device;
mod desk;
mod local_client;
mod pair_device;
mod phone;
mod remove_device;
mod start_pairing;

pub use check_device::CheckDevice;
pub use desk::PairingDesk;
pub use local_client::LocalClient;
pub use pair_device::PairDevice;
pub use phone::{Phone, PhoneError, PhoneGate, PhoneReply, PhoneRequest};
pub use remove_device::RemoveDevice;
pub use start_pairing::StartPairing;

#[cfg(test)]
pub(crate) mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use udm_domain::address::Address;
    use udm_domain::download::{DownloadEntry, Status};
    use udm_domain::id::{DeviceId, DownloadId};
    use udm_domain::pairing::{Device, DeviceSecret, PairingError, Verifier, RANDOM_BYTES};
    use udm_domain::ports::{DevicesError, PairedDevices, PairingKeys, Timestamps};
    use udm_domain::queue::Queue;

    use super::*;
    use crate::daemon::{ClientId, Command, Event, Failure, Reply, Requests};
    use crate::memory::row;
    use crate::remote::RemoteError;

    pub fn peer(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, n))
    }

    /// Counting "random" bytes, and a digest that reverses them.
    #[derive(Default)]
    pub struct Keys {
        pub broken: bool,
        next: AtomicU64,
    }

    impl Keys {
        pub fn broken() -> Self {
            Keys { broken: true, ..Keys::default() }
        }
    }

    impl PairingKeys for Keys {
        fn random(&self) -> Result<[u8; RANDOM_BYTES], DevicesError> {
            if self.broken {
                return Err(DevicesError("no randomness".into()));
            }
            let n = self.next.fetch_add(1, Ordering::SeqCst);
            let mut r = [0u8; RANDOM_BYTES];
            r[..8].copy_from_slice(&n.to_le_bytes());
            r[8..16].copy_from_slice(&(n.wrapping_mul(0x9e37_79b9_7f4a_7c15)).to_le_bytes());
            Ok(r)
        }

        fn verifier(&self, secret: &DeviceSecret) -> Verifier {
            let mut d = *secret.bytes();
            d.reverse();
            Verifier::new(d)
        }
    }

    /// The device list in memory.
    #[derive(Default)]
    pub struct Book {
        pub broken: bool,
        devices: Mutex<Vec<Device>>,
    }

    impl Book {
        pub fn broken() -> Self {
            Book { broken: true, ..Book::default() }
        }

        pub fn saved(&self) -> Vec<Device> {
            self.devices.lock().unwrap().clone()
        }
    }

    impl PairedDevices for Book {
        fn load(&self) -> Result<Vec<Device>, DevicesError> {
            Ok(self.saved())
        }

        fn save(&self, devices: &[Device]) -> Result<(), DevicesError> {
            if self.broken {
                return Err(DevicesError("disk full".into()));
            }
            *self.devices.lock().unwrap() = devices.to_vec();
            Ok(())
        }
    }

    struct At(u64);

    impl Timestamps for At {
        fn unix_now(&self) -> u64 {
            self.0
        }
    }

    /// The daemon's service as far as the page reaches it: a list of two
    /// downloads, and every command it was handed.
    #[derive(Default)]
    struct Daemon {
        handled: Mutex<Vec<(ClientId, Command)>>,
        gone: Mutex<Vec<ClientId>>,
    }

    impl Requests for Daemon {
        fn handle(&self, client: ClientId, command: Command, reply: &Reply) {
            self.handled.lock().unwrap().push((client, command.clone()));
            match command {
                Command::Ping => {
                    reply.send(Event::Pong);
                }
                Command::List => {
                    let items: Vec<DownloadEntry> = vec![row(1, Status::Downloading), row(2, Status::Done)];
                    reply.send(Event::Downloads { items });
                    reply.send(Event::Queues { items: vec![Queue::main()] });
                }
                Command::Add(d) => {
                    reply.send(Event::Added { id: DownloadId::new(9), url: d.url.clone() });
                    reply.send(Event::Ack { id: DownloadId::new(9), url: d.url, status: "accepted".into() });
                }
                Command::Retry { id } => {
                    reply.send(Event::Ack { id: DownloadId::new(id.get() + 10), url: "u".into(), status: "ok".into() });
                }
                Command::Remove { id, .. } if id.get() == 1 => {
                    reply.send(Event::Error { error: Failure::Other("it is running".into()) });
                }
                _ => {}
            }
        }

        fn gone(&self, client: ClientId) {
            self.gone.lock().unwrap().push(client);
        }
    }

    fn phone() -> (Phone, Arc<PairingDesk>, Arc<Daemon>) {
        let desk = Arc::new(PairingDesk::open(Arc::new(Book::default()), Arc::new(Keys::default()), Arc::new(At(50))));
        let daemon = Arc::new(Daemon::default());
        let ids = AtomicU64::new(1000);
        let phone = Phone::new(
            Arc::clone(&desk),
            Arc::clone(&daemon) as _,
            Box::new(move || ClientId::new(ids.fetch_add(1, Ordering::SeqCst))),
        );
        (phone, desk, daemon)
    }

    fn paired(phone: &Phone, desk: &PairingDesk) -> String {
        let code = desk.start().unwrap().code;
        phone.pair(code.as_str(), "Phone", peer(4)).unwrap().to_text()
    }

    #[test]
    fn a_paired_phone_lists_adds_pauses_resumes_and_removes_through_the_daemons_commands() {
        let (phone, desk, daemon) = phone();
        let me = paired(&phone, &desk);
        let Ok(PhoneReply::Listing(listing)) = phone.run(&me, peer(4), PhoneRequest::List) else {
            panic!("no list");
        };
        assert_eq!(listing.downloads.len(), 2);
        let url = Address::parse("https://dl.example/a.iso").unwrap();
        assert_eq!(phone.run(&me, peer(4), PhoneRequest::Add(url)), Ok(PhoneReply::Added(DownloadId::new(9))));
        assert_eq!(phone.run(&me, peer(4), PhoneRequest::Pause(DownloadId::new(1))), Ok(PhoneReply::Done));
        assert_eq!(phone.run(&me, peer(4), PhoneRequest::Resume(DownloadId::new(2))), Ok(PhoneReply::Done));
        let remove = PhoneRequest::Remove { id: DownloadId::new(2), delete_files: false };
        assert_eq!(phone.run(&me, peer(4), remove), Ok(PhoneReply::Done));
        let refused = phone.run(&me, peer(4), PhoneRequest::Remove { id: DownloadId::new(1), delete_files: false });
        assert_eq!(refused, Err(PhoneError::Remote(RemoteError::Refused(Failure::Other("it is running".into())))));

        let handled = daemon.handled.lock().unwrap().clone();
        let commands: Vec<&Command> = handled.iter().map(|(_, c)| c).collect();
        let add = commands.iter().find_map(|c| match c {
            Command::Add(d) => Some(d.clone()),
            _ => None,
        });
        let add = add.unwrap();
        assert_eq!((add.folder, add.filename, add.ask, add.start), (None, None, Some(false), Some(true)));
        assert!(commands.iter().any(|c| matches!(c, Command::Cancel { id } if id.get() == 1)));
        assert!(commands.iter().any(|c| matches!(c, Command::Retry { id } if id.get() == 2)));
        assert!(commands.iter().any(|c| matches!(c, Command::Remove { id, delete_files: false } if id.get() == 2)));
        assert!(!commands.iter().any(|c| matches!(c, Command::Remove { delete_files: true, .. })), "files stay");
        // Each request had a connection of its own, which went when it was done.
        let mut clients: Vec<ClientId> = handled.iter().map(|(c, _)| *c).collect();
        clients.dedup();
        let mut gone = daemon.gone.lock().unwrap().clone();
        gone.sort();
        assert_eq!(clients, gone);
        assert!(clients.iter().all(|c| c.get() >= 1000));
    }

    #[test]
    fn deleting_files_from_the_phone_is_asked_for_explicitly() {
        let (phone, desk, daemon) = phone();
        let me = paired(&phone, &desk);
        let remove = PhoneRequest::Remove { id: DownloadId::new(2), delete_files: true };
        assert_eq!(phone.run(&me, peer(4), remove), Ok(PhoneReply::Done));
        assert!(daemon
            .handled
            .lock()
            .unwrap()
            .iter()
            .any(|(_, c)| matches!(c, Command::Remove { delete_files: true, .. })));
    }

    #[test]
    fn nothing_reaches_the_daemon_without_a_paired_credential() {
        let (phone, desk, daemon) = phone();
        for credential in ["", "nonsense", &format!("1.{}", "a".repeat(64))] {
            assert_eq!(
                phone.run(credential, peer(4), PhoneRequest::List),
                Err(PhoneError::Pairing(PairingError::NotPaired))
            );
        }
        let me = paired(&phone, &desk);
        desk.remove(DeviceId::new(1)).unwrap();
        assert_eq!(phone.run(&me, peer(5), PhoneRequest::List), Err(PhoneError::Pairing(PairingError::NotPaired)));
        assert!(daemon.handled.lock().unwrap().is_empty());
    }

    #[test]
    fn a_phone_can_unpair_itself() {
        let (phone, desk, _daemon) = phone();
        let me = paired(&phone, &desk);
        assert_eq!(phone.run(&me, peer(4), PhoneRequest::Unpair), Ok(PhoneReply::Done));
        assert!(desk.devices().is_empty());
        assert_eq!(phone.run(&me, peer(4), PhoneRequest::List), Err(PhoneError::Pairing(PairingError::NotPaired)));
    }

    #[test]
    fn the_desk_reads_the_devices_kept_and_lists_them_without_verifiers() {
        let book = Arc::new(Book::default());
        book.save(&[Device {
            id: DeviceId::new(3),
            name: udm_domain::pairing::DeviceName::clean("Tablet"),
            paired_at: 7,
            last_seen: None,
            verifier: Verifier::new([2; RANDOM_BYTES]),
        }])
        .unwrap();
        let desk = PairingDesk::open(book, Arc::new(Keys::default()), Arc::new(At(50)));
        let devices = desk.devices();
        assert_eq!((devices.len(), devices[0].id, devices[0].name.as_str()), (1, DeviceId::new(3), "Tablet"));
        assert!(desk.pending().is_none());
        desk.start().unwrap();
        assert!(desk.pending().is_some());
        desk.cancel();
        assert!(desk.pending().is_none());
    }
}
