use crate::foreground;
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, Stream, ToNsName,
    traits::{ListenerExt, Stream as LocalSocketStreamExt},
};
use noteiq_core::{
    launch::{forward_message, parse_forward_message},
    model::CaptureSource,
};
use std::{
    io::{BufRead, BufReader, Write},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const SOCKET_NAME: &str = "noteiq-quick-capture";
const CONNECT_ATTEMPTS: u32 = 10;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(50);

pub enum InstanceRole {
    Primary(Receiver<CaptureSource>),
    Forwarded,
}

pub fn start_or_forward(source: CaptureSource) -> Result<InstanceRole, String> {
    start_or_forward_named(SOCKET_NAME, source)
}

fn start_or_forward_named(
    socket_name: &str,
    source: CaptureSource,
) -> Result<InstanceRole, String> {
    let name = socket_name
        .to_ns_name::<GenericNamespaced>()
        .map_err(|_| "NoteIQ could not initialize its single-instance channel.")?;
    // On Windows a second listener on an existing pipe fails with access denied rather than
    // "address in use", so any creation failure means "try the running instance first".
    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => listener,
        Err(_) => {
            forward_to_running_instance(socket_name, source)?;
            return Ok(InstanceRole::Forwarded);
        }
    };

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            let mut request = String::new();
            if BufReader::new(connection).read_line(&mut request).is_ok()
                && let Some(source) = parse_forward_message(&request)
            {
                let _ = sender.send(source);
            }
        }
    });
    Ok(InstanceRole::Primary(receiver))
}

fn forward_to_running_instance(socket_name: &str, source: CaptureSource) -> Result<(), String> {
    let mut connection = None;
    for attempt in 0..CONNECT_ATTEMPTS {
        let name = socket_name
            .to_ns_name::<GenericNamespaced>()
            .map_err(|_| "NoteIQ could not reach its open window.")?;
        match Stream::connect(name) {
            Ok(stream) => {
                connection = Some(stream);
                break;
            }
            Err(_) if attempt + 1 < CONNECT_ATTEMPTS => thread::sleep(CONNECT_RETRY_DELAY),
            Err(_) => {}
        }
    }
    let mut connection = connection.ok_or(
        "NoteIQ could not reach its open window. Quit NoteIQ from the tray and try again.",
    )?;
    foreground::allow_running_instance_to_activate();
    connection
        .write_all(forward_message(source).as_bytes())
        .and_then(|()| connection.write_all(b"\n"))
        .map_err(|_| "NoteIQ could not send the request to its open window.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_forwards_its_source_to_the_running_instance() {
        let name = format!("noteiq-test-{}", std::process::id());
        let InstanceRole::Primary(receiver) =
            start_or_forward_named(&name, CaptureSource::Tray).expect("primary starts")
        else {
            panic!("the first launch must become the primary instance");
        };

        for source in [CaptureSource::Cli, CaptureSource::Uri] {
            let role = start_or_forward_named(&name, source).expect("second launch forwards");
            assert!(matches!(role, InstanceRole::Forwarded));
            let received = receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("running instance receives the request");
            assert_eq!(received, source);
        }
    }
}
