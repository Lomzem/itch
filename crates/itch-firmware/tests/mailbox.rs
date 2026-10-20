use std::pin::pin;
use std::task::Poll;

use embassy_futures::{block_on, poll_once};
use itch_core::frame::{FrameDecoder, MAX_FRAME};
use itch_core::rig::LogControl;
use itch_core::wire::LogLevel;
use itch_firmware::mailbox::{Mailbox, Payload};
use log::Level;

// Links the host's critical section, which the mailbox locks with.
use critical_section as _;

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
    let mailbox = Mailbox::new();
    mailbox.set_log_level(LogLevel::Info);
    mailbox.log(Level::Info, format_args!("first")).unwrap();
    block_on(mailbox.reply(Payload::new(b"reply")));
    assert_eq!(next(&mailbox).as_deref(), Some(&b"reply"[..]));
    assert!(next(&mailbox).unwrap().ends_with(b"first"));
    assert_eq!(next(&mailbox), None);
}

#[test]
fn logs_start_off_and_a_level_change_clears_backlog_and_filters_future_events() {
    let mailbox = Mailbox::new();
    mailbox.log(Level::Error, format_args!("initially off")).unwrap();
    assert_eq!(next(&mailbox), None);

    mailbox.set_log_level(LogLevel::Debug);
    mailbox.log(Level::Info, format_args!("backlog")).unwrap();
    mailbox.set_log_level(LogLevel::Warn);
    mailbox.log(Level::Info, format_args!("filtered")).unwrap();
    mailbox.log(Level::Error, format_args!("current")).unwrap();
    assert!(next(&mailbox).unwrap().ends_with(b"current"));
    assert_eq!(next(&mailbox), None);
}

#[test]
fn a_reply_waits_for_room_until_the_writer_consumes_one() {
    let mailbox = Mailbox::new();
    block_on(mailbox.reply(Payload::new(b"1")));
    block_on(mailbox.reply(Payload::new(b"2")));
    let mut waiting = pin!(mailbox.reply(Payload::new(b"3")));
    assert_eq!(poll_once(waiting.as_mut()), Poll::Pending);

    assert_eq!(next(&mailbox).as_deref(), Some(&b"1"[..]));
    assert_eq!(poll_once(waiting.as_mut()), Poll::Ready(()));
    assert_eq!(next(&mailbox).as_deref(), Some(&b"2"[..]));
    assert_eq!(next(&mailbox).as_deref(), Some(&b"3"[..]));
    assert_eq!(next(&mailbox), None);
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
        let mailbox = Mailbox::new();
        for request_id in 0..4 {
            mailbox.submit(Ok(&[0x01, request_id, 0, 0, 0]));
        }
        mailbox.submit(Ok(request));
        assert_eq!(next(&mailbox), busy, "{request:02x?}");
        for request_id in 0..4 {
            let Poll::Ready(Ok(payload)) = poll_once(mailbox.next_request()) else {
                panic!("the earlier request {request_id} must remain queued");
            };
            assert_eq!(payload.as_bytes(), &[0x01, request_id, 0, 0, 0]);
        }
        assert!(matches!(poll_once(mailbox.next_request()), Poll::Pending));
    }
}
