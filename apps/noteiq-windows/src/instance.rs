use crate::foreground;
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, Stream, ToNsName,
    traits::{ListenerExt, Stream as LocalSocketStreamExt},
};
use noteiq_core::launch::{ForwardRequest, forward_request_message, parse_forward_request};
use std::{
    io::{Read, Write},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const SOCKET_NAME: &str = "noteiq-quick-capture";
const CONNECT_ATTEMPTS: u32 = 10;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(50);
const MAX_FORWARD_MESSAGE_BYTES: u64 = 256;
const FORWARD_READ_TIMEOUT: Duration = Duration::from_secs(2);
const FORWARD_READ_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub enum InstanceRole {
    Primary(Receiver<ForwardRequest>),
    Forwarded,
}

pub fn start_or_forward(request: ForwardRequest) -> Result<InstanceRole, String> {
    let mut session = 0;
    // SAFETY: the process ID is ours and `session` is a valid writable u32.
    if unsafe {
        windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
            std::process::id(),
            &mut session,
        )
    } == 0
    {
        return Err("NoteIQ could not identify its Windows session.".into());
    }
    start_or_forward_named(&session_socket_name(session), request)
}

fn session_socket_name(session: u32) -> String {
    format!("{SOCKET_NAME}-{session}")
}

fn start_or_forward_named(
    socket_name: &str,
    request: ForwardRequest,
) -> Result<InstanceRole, String> {
    let name = socket_name
        .to_ns_name::<GenericNamespaced>()
        .map_err(|_| "NoteIQ could not initialize its single-instance channel.")?;
    // On Windows a second listener on an existing pipe fails with access denied rather than
    // "address in use", so any creation failure means "try the running instance first".
    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => listener,
        Err(_) => {
            forward_to_running_instance(socket_name, &request)?;
            return Ok(InstanceRole::Forwarded);
        }
    };

    let (sender, receiver) = mpsc::channel();
    // A reminder that started NoteIQ is handled like one forwarded to the running instance.
    if matches!(request, ForwardRequest::MeetingReminder(_)) {
        let _ = sender.send(request);
    }
    thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            if let Some(request) = read_forward_request(connection) {
                let _ = sender.send(request);
            }
        }
    });
    Ok(InstanceRole::Primary(receiver))
}

fn read_forward_request(mut connection: Stream) -> Option<ForwardRequest> {
    connection.set_nonblocking(true).ok()?;
    let started = Instant::now();
    let mut request = Vec::new();
    loop {
        let remaining = FORWARD_READ_TIMEOUT.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return None;
        }
        let mut byte = [0];
        match connection.read(&mut byte) {
            Ok(0) => thread::sleep(FORWARD_READ_POLL_INTERVAL.min(remaining)),
            Ok(_) if byte[0] == b'\n' => {
                let request = std::str::from_utf8(&request).ok()?;
                return parse_forward_request(request);
            }
            Ok(_) if request.len() >= MAX_FORWARD_MESSAGE_BYTES as usize => return None,
            Ok(_) => request.push(byte[0]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(FORWARD_READ_POLL_INTERVAL.min(remaining));
            }
            Err(_) => return None,
        }
    }
}

fn forward_to_running_instance(socket_name: &str, request: &ForwardRequest) -> Result<(), String> {
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
        .write_all(forward_request_message(request).as_bytes())
        .and_then(|()| connection.write_all(b"\n"))
        .map_err(|_| "NoteIQ could not send the request to its open window.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use noteiq_core::model::CaptureSource;

    #[test]
    fn windows_sessions_have_separate_channels() {
        assert_ne!(session_socket_name(1), session_socket_name(2));
        assert_eq!(session_socket_name(1), session_socket_name(1));
    }

    #[test]
    fn second_launch_forwards_its_source_to_the_running_instance() {
        let name = format!("noteiq-test-{}", std::process::id());
        let InstanceRole::Primary(receiver) =
            start_or_forward_named(&name, ForwardRequest::Show(CaptureSource::Tray))
                .expect("primary starts")
        else {
            panic!("the first launch must become the primary instance");
        };

        for source in [CaptureSource::Cli, CaptureSource::Uri] {
            let role = start_or_forward_named(&name, ForwardRequest::Show(source))
                .expect("second launch forwards");
            assert!(matches!(role, InstanceRole::Forwarded));
            let received = receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("running instance receives the request");
            assert_eq!(received, ForwardRequest::Show(source));
        }
    }

    #[test]
    fn reminder_clicks_reach_the_running_or_starting_instance() {
        let name = format!("noteiq-reminder-{}", std::process::id());
        let reminder = ForwardRequest::MeetingReminder("0123456789abcdef".to_owned());
        let InstanceRole::Primary(receiver) =
            start_or_forward_named(&name, reminder.clone()).expect("primary starts")
        else {
            panic!("the first launch must become the primary instance");
        };
        assert_eq!(receiver.try_recv().unwrap(), reminder);

        let role = start_or_forward_named(&name, reminder.clone()).expect("reminder forwards");
        assert!(matches!(role, InstanceRole::Forwarded));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            reminder
        );
    }

    #[test]
    fn incomplete_client_cannot_block_later_launches() {
        let name = format!("noteiq-incomplete-{}", std::process::id());
        let InstanceRole::Primary(receiver) =
            start_or_forward_named(&name, ForwardRequest::Show(CaptureSource::Tray))
                .expect("primary starts")
        else {
            panic!("the first launch must become the primary instance");
        };
        let socket = name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let mut incomplete = Stream::connect(socket).expect("connect incomplete client");
        incomplete.write_all(b"{").expect("write partial request");

        let started = Instant::now();
        let role = start_or_forward_named(&name, ForwardRequest::Show(CaptureSource::Cli))
            .expect("later launch forwards after the incomplete request times out");

        assert!(matches!(role, InstanceRole::Forwarded));
        assert_eq!(
            receiver
                .recv_timeout(FORWARD_READ_TIMEOUT + Duration::from_secs(2))
                .unwrap(),
            ForwardRequest::Show(CaptureSource::Cli)
        );
        assert!(started.elapsed() < FORWARD_READ_TIMEOUT + Duration::from_secs(2));
    }

    #[test]
    fn oversized_request_is_discarded_without_blocking_later_launches() {
        let name = format!("noteiq-oversized-{}", std::process::id());
        let InstanceRole::Primary(receiver) =
            start_or_forward_named(&name, ForwardRequest::Show(CaptureSource::Tray))
                .expect("primary starts")
        else {
            panic!("the first launch must become the primary instance");
        };
        let socket = name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let mut oversized = Stream::connect(socket).expect("connect oversized client");
        oversized
            .write_all(&vec![b'x'; MAX_FORWARD_MESSAGE_BYTES as usize + 1])
            .expect("write oversized request");
        drop(oversized);

        let role = start_or_forward_named(&name, ForwardRequest::Show(CaptureSource::Uri))
            .expect("later launch forwards after discarding the oversized request");

        assert!(matches!(role, InstanceRole::Forwarded));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            ForwardRequest::Show(CaptureSource::Uri)
        );
    }
}
