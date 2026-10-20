mod common;

use common::{FakeBoard, TestRig};
use itch_core::wire::{ErrorCode, Reply, Request, SetLabel};

fn set_label(label: &str) -> Request<'_> {
    Request::SetLabel(SetLabel { holder: "", label })
}

fn hello_label(rig: &mut TestRig) -> String {
    match rig.ok(Request::Hello) {
        Reply::Hello(hello) => hello.label.to_owned(),
        other => panic!("not a Hello reply: {other:?}"),
    }
}

#[test]
fn set_label_saves_the_label_and_hello_reports_it() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(set_label("bench3")), Reply::SetLabel { label: "bench3" });
    assert_eq!(hello_label(&mut rig), "bench3");
    assert_eq!(rig.board.saved, ["bench3"]);
}

#[test]
fn an_empty_label_restores_the_default_without_saving_it() {
    let mut rig = TestRig::new();
    rig.ok(set_label("bench3"));
    assert_eq!(rig.ok(set_label("")), Reply::SetLabel { label: "itch-C02718" });
    assert_eq!(hello_label(&mut rig), "itch-C02718");
    assert_eq!(rig.board.saved, ["bench3", ""]);
}

#[test]
fn setting_the_default_label_by_name_saves_no_custom_label() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(set_label("itch-C02718")), Reply::SetLabel { label: "itch-C02718" });
    assert_eq!(rig.board.saved, [""]);
}

#[test]
fn a_label_of_64_bytes_is_accepted_and_65_is_malformed() {
    let mut rig = TestRig::new();
    let longest = "é".repeat(32);
    assert_eq!(rig.ok(set_label(&longest)), Reply::SetLabel { label: &longest });
    assert_eq!(rig.refused(set_label(&"a".repeat(65))), ErrorCode::Malformed);
    assert_eq!(hello_label(&mut rig), longest);
}

#[test]
fn a_save_failure_is_a_storage_fault_and_keeps_the_old_label() {
    let mut rig = TestRig::new();
    rig.ok(set_label("bench3"));
    rig.board.storage_fault = true;
    assert_eq!(rig.refused(set_label("bench4")), ErrorCode::StorageFault);
    assert_eq!(rig.refused(set_label("")), ErrorCode::StorageFault);
    assert_eq!(hello_label(&mut rig), "bench3");
}

#[test]
fn the_rig_reports_the_label_stored_at_boot() {
    let mut board = FakeBoard::new();
    board.stored = Some("bench3".into());
    assert_eq!(hello_label(&mut TestRig::on(board)), "bench3");
}

#[test]
fn no_stored_label_or_an_empty_one_means_the_default() {
    assert_eq!(hello_label(&mut TestRig::new()), "itch-C02718");
    let mut board = FakeBoard::new();
    board.stored = Some(String::new());
    assert_eq!(hello_label(&mut TestRig::on(board)), "itch-C02718");
}
