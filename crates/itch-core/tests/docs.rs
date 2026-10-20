mod common;

use common::{cases, hex, protocol_dir, vectors};

/// Every ```hex block in the protocol docs must be the payload, frame or
/// stream of some vector, so the docs can't go stale.
#[test]
fn every_hex_block_in_the_protocol_docs_matches_a_vector() {
    let vectors = vectors();
    let known: Vec<Vec<u8>> = [
        ("messages", "payload"),
        ("frames", "payload"),
        ("frames", "frame"),
        ("streams", "stream"),
        ("malformed", "payload"),
    ]
    .iter()
    .flat_map(|&(section, field)| {
        cases(&vectors, section).iter().map(move |case| hex(case[field].as_str().unwrap()))
    })
    .collect();

    let mut checked = 0;
    for entry in std::fs::read_dir(protocol_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        for block in hex_blocks(&std::fs::read_to_string(&path).unwrap()) {
            assert!(known.contains(&hex(&block)), "{}: no vector matches\n{block}", path.display());
            checked += 1;
        }
    }
    assert!(checked > 0, "found no hex blocks");
}

fn hex_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim()) {
            (None, "```hex") => current = Some(String::new()),
            (Some(block), "```") => {
                blocks.push(std::mem::take(block));
                current = None;
            }
            (Some(block), text) => {
                block.push_str(text);
                block.push('\n');
            }
            (None, _) => {}
        }
    }
    assert!(current.is_none(), "unterminated hex block");
    blocks
}
