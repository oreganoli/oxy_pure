use imap::{connect, Client, Session};
use native_tls::{TlsConnector, TlsStream};
use std::error::Error;
use std::net::TcpStream;
use std::fmt::Display;
use rustyknife::rfc2047::encoded_word;
use imap::types::{Mailbox, Fetch};
use super::ImapError;
type ImapSession = Session<TlsStream<TcpStream>>;

const SERVER: &str = "poczta.o2.pl";
const PORT: u16 = 993;

pub struct Purifier {
    /// The username used to connect to the server.
    username: String,
    /// The password used to connect to the server.
    password: String,
    /// How many e-mails have been deleted.
    counter: usize,
}

impl Purifier {
    pub fn new() -> Self {
        use std::env::var;
        Self {
            username: var("O2_USERNAME").expect("The O2_USERNAME environment variable must be set."),
            password: var("O2_PASSWORD").expect("The O2_PASSWORD environment variable must be set."),
            counter: 0
        }
    }
    fn session(&self) -> ImapSession {
        let tls = TlsConnector::builder().build().unwrap();
        let client = match connect((SERVER, PORT), SERVER, &tls) {
            Ok(client) => {
                info!("Successfully connected to poczta.o2.pl");
                client
            },
            Err(e) => {
                error!("Failed to connect to poczta.o2.pl, exiting!");
                error!("Error message: {:#?}", e);
                std::process::exit(1)
            }
        };
        match client.login(&self.username, &self.password) {
            Ok(session) => {
                info!("Successfully authenticated.");
                session
            },
            Err(e) => {
                error!("Could not log in, exiting!");
                error!("Error message: {:#?}", e);
                std::process::exit(2)
            }
        }
    }
    fn get_spam_uids(&self, session: &mut ImapSession) -> Result<String, ImapError> {
        get_inbox(session)?;
        let mut sequence = String::new();
        info!("Fetching messages...");
        for msg in session
            .fetch("1:*", "UID ENVELOPE")?
            .into_iter() {
            let envelope = match msg.envelope() {
                Some(env) => env,
                None => continue
            };
            let from = match &envelope.from {
                Some(from) => from,
                None => continue
            };
            let uid = match msg.uid {
                Some(u) => u,
                None => continue
            };
            for address in from {
                if let Some(name) = address.name {
                    let (_, name) = encoded_word(name.as_bytes()).unwrap();
                    if name.contains("/o2") || name.contains("/ o2") {
                        info!("Found a spam message with the sender name {:?} and UID {}", name, uid);
                        sequence.push_str(&format!("{},", uid));
                    }
                }
            }
        }
        sequence.pop(); // remove the last trailing comma
        Ok(sequence)
    }
    fn delete_messages(&mut self, sequence: &str, session: &mut ImapSession) -> Result<(), ImapError> {
        if sequence.is_empty() {
            return Ok(())
        }
        let mut set = String::new();
        let mut count = 0usize;
        count += sequence.split(",").count();
        set.pop(); // remove the final trailing comma
        info!("Setting the Deleted flag on {} messages...", count);
        session.uid_store(sequence, "+FLAGS (\\Deleted)")?;
        info!("Expunging the mailbox...");
        session.expunge()?;
        info!("Success, {} messages permanently deleted.", count);
        self.counter += count;
        Ok(())
    }
    pub fn run(&mut self) -> Result<(), ImapError> {
        info!("Acquiring session...");
        let session = &mut self.session();
        let spam = self.get_spam_uids(session)?;
        self.delete_messages(&spam, session)?;
        info!("Logging out...");
        session.logout().ok(); // the imap crate can't handle an "a4 BYE IMAP4rev1 Server logging out" response
        Ok(())
    }
}

fn get_inbox(session: &mut ImapSession) -> Result<Mailbox, ImapError> {
    match session.select("INBOX") {
        Ok(mailbox) => {
            info!("Successfully got the inbox.");
            Ok(mailbox)
        },
        Err(e) => {
            match e {
                ImapError::No(msg) => {
                    warn!("The IMAP server responded with a NO. You should probably try again later");
                    warn!("Error message: {}", &msg);
                    return Err(ImapError::No(msg))
                },
                _ => {
                    error!("The INBOX mailbox could not be gotten.");
                    error!("Error message: {:#?}", e);
                    std::process::exit(4);
                }
            }
        }
    }
}