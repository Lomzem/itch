use std::pin::pin;
use std::task::Poll;

use embassy_futures::{block_on, poll_once};
use itch_core::frame::{FrameDecoder, MAX_FRAME};
use itch_core::rig::Connection;
use itch_core::wire::LogLevel;
use itch_firmware::mailbox::{DEPTH, Mailbox, Payload, Requests};
use log::Level;

// Links the host's critical section, which the mailbox locks with.
use critical_section as _;

/// A mailbox of its own, on a request queue of its own.
fn mailbox() -> (&'static Mailbox, &'static Requests) {
    let requests = Box::leak(Box::new(Requests::new()));
    (Box::leak(Box::new(Mailbox::new(requests))), requests)
}

/// The payload of the mailbox's next outgoing frame, or `None` if it has
/// none ready.
fn next(mailbox: &Mailbox) -> Option<Vec<u8>> {
    let mut frame = [0; MAX_FRAME];
    let Poll::Ready(len) = poll_once(mailbox.next_frame(&mut frame)) else {
        return None;
    };
    let mut decoder = FrameDecoder::new();
    let payload =
        frame[..len].iter().find_map(|&byte| decoder.push(byte)?.ok().map(<[u8]>::to_vec));
    Some(payload.expect("a whole frame"))
}

#[test]
fn a_reply_goes_out_before_log_events_queued_ahead_of_it() {
    let (mailbox, _) = mailbox();
    let origin = mailbox.connect();
    origin.set_log_level(LogLevel::Info);
    mailbox.log(Level::Info, format_args!("first")).unwrap();
    block_on(origin.reply(Payload::new(b"reply")));
    assert_eq!(next(mailbox).as_deref(), Some(&b"reply"[..]));
    assert!(next(mailbox).unwrap().ends_with(b"first"));
    assert_eq!(next(mailbox), None);
}

#[test]
fn a_new_connection_starts_with_logs_off_and_gets_nothing_meant_for_the_last() {
    let (mailbox, _) = mailbox();
    let earlier = mailbox.connect();
    earlier.set_log_level(LogLevel::Debug);
    block_on(earlier.reply(Payload::new(b"queued")));

    let _current = mailbox.connect();
    // The rig task answers requests from the earlier connection.
    earlier.set_log_level(LogLevel::Debug);
    block_on(earlier.reply(Payload::new(b"late")));
    mailbox.log(Level::Error, format_args!("not for anyone yet")).unwrap();
    assert_eq!(next(mailbox), None);
}

#[test]
fn a_reply_waiting_for_room_is_dropped_when_its_connection_ends() {
    let (mailbox, _) = mailbox();
    let origin = mailbox.connect();
    block_on(origin.reply(Payload::new(b"1")));
    block_on(origin.reply(Payload::new(b"2")));
    let mut waiting = pin!(origin.reply(Payload::new(b"3")));
    assert_eq!(poll_once(waiting.as_mut()), Poll::Pending);

    mailbox.disconnect();
    assert_eq!(poll_once(waiting.as_mut()), Poll::Ready(()));
    assert_eq!(next(mailbox), None);
}

/// A request that finds the queue full is answered `Busy` at once, unless
/// it has no `request_id` to answer by.
#[test]
fn a_request_that_finds_the_queue_full_is_answered_busy() {
    // GetChannels, request_id 0x12345678, and the same cut short.
    let cases: [(&[u8], _); 2] = [
        (&[0x02, 0x78, 0x56, 0x34, 0x12], Some(vec![0x82, 0x78, 0x56, 0x34, 0x12, 0x07, 0x00])),
        (&[0x02, 0x78, 0x56, 0x34], None),
    ];
    for (request, busy) in cases {
        let (mailbox, requests) = mailbox();
        let origin = mailbox.connect();
        for _ in 0..DEPTH {
            origin.receive(Ok(&[0x01, 0, 0, 0, 0]));
        }
        origin.receive(Ok(request));
        assert_eq!(next(mailbox), busy, "{request:02x?}");
        assert_eq!(requests.len(), DEPTH);
    }
}
