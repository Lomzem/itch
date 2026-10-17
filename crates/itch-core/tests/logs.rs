use itch_core::frame::MAX_PAYLOAD;
use itch_core::logs::{LogDropped, LogQueue};
use itch_core::wire::{Event, LogEvent, LogLevel, Message};
use log::Level;

#[test]
fn logs_are_off_until_the_level_is_set() {
    let mut queue = LogQueue::<2048>::new();
    assert_eq!(queue.push(Level::Error, format_args!("expander fault")), Ok(()));
    assert_eq!(pop(&mut queue), None);
}

#[test]
fn queues_lines_at_or_above_the_level() {
    let mut queue = LogQueue::<2048>::new();
    queue.set_level(LogLevel::Warn);
    for level in [Level::Debug, Level::Info, Level::Warn, Level::Error] {
        queue.push(level, format_args!("{level} line")).unwrap();
    }
    assert_eq!(pop(&mut queue), Some((LogLevel::Warn, 0, "WARN line".into())));
    assert_eq!(pop(&mut queue), Some((LogLevel::Error, 0, "ERROR line".into())));
    assert_eq!(pop(&mut queue), None);
}

#[test]
fn a_full_queue_drops_lines_and_the_next_event_counts_them() {
    let mut queue = LogQueue::<1100>::new();
    queue.set_level(LogLevel::Debug);
    let long = "x".repeat(1000);
    queue.push(Level::Info, format_args!("{long}")).unwrap();
    assert_eq!(queue.push(Level::Info, format_args!("{long}")), Err(LogDropped));
    assert_eq!(queue.push(Level::Info, format_args!("{long}")), Err(LogDropped));
    assert_eq!(pop(&mut queue), Some((LogLevel::Info, 2, long.clone())));
    queue.push(Level::Warn, format_args!("after")).unwrap();
    assert_eq!(pop(&mut queue), Some((LogLevel::Warn, 0, "after".into())));
}

#[test]
fn a_long_line_is_truncated_to_fill_the_frame_on_a_character_boundary() {
    let mut queue = LogQueue::<2048>::new();
    queue.set_level(LogLevel::Info);
    // 1017 ASCII bytes leave room for one byte of the two-byte `é`.
    let line = format!("{}é tail", "a".repeat(1017));
    queue.push(Level::Info, format_args!("{line}")).unwrap();
    let mut out = [0; MAX_PAYLOAD];
    let len = queue.pop(&mut out).unwrap();
    let Ok(Message::Event(Event::Log(LogEvent { text, .. }))) = Message::decode(&out[..len]) else {
        panic!("not a Log Event");
    };
    assert_eq!(text, "a".repeat(1017));

    queue.push(Level::Info, format_args!("{}", "b".repeat(2000))).unwrap();
    assert_eq!(queue.pop(&mut out), Some(MAX_PAYLOAD));
}

#[test]
fn lines_survive_wrapping_around_the_queue() {
    let mut queue = LogQueue::<1100>::new();
    queue.set_level(LogLevel::Info);
    for n in 0..200 {
        let line = format!("line {n} {}", "y".repeat(n * 5));
        queue.push(Level::Info, format_args!("{line}")).unwrap();
        assert_eq!(pop(&mut queue), Some((LogLevel::Info, 0, line)));
    }
}

#[test]
fn setting_the_level_discards_queued_lines_and_drops() {
    let mut queue = LogQueue::<1100>::new();
    queue.set_level(LogLevel::Info);
    queue.push(Level::Info, format_args!("{}", "z".repeat(1000))).unwrap();
    assert_eq!(queue.push(Level::Info, format_args!("{}", "z".repeat(1000))), Err(LogDropped));
    queue.set_level(LogLevel::Debug);
    assert_eq!(pop(&mut queue), None);
    queue.push(Level::Debug, format_args!("fresh")).unwrap();
    assert_eq!(pop(&mut queue), Some((LogLevel::Debug, 0, "fresh".into())));
}

fn pop<const BYTES: usize>(queue: &mut LogQueue<BYTES>) -> Option<(LogLevel, u32, String)> {
    let mut out = [0; MAX_PAYLOAD];
    let len = queue.pop(&mut out)?;
    match Message::decode(&out[..len]) {
        Ok(Message::Event(Event::Log(LogEvent { level, dropped, text }))) => {
            Some((level, dropped, text.to_owned()))
        }
        other => panic!("not a Log Event: {other:?}"),
    }
}
