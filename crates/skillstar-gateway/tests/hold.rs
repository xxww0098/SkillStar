use std::cell::Cell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use serde_json::Value;
use skillstar_gateway::HoldWriter;

#[test]
fn hold_swaps_upstream_when_error_precedes_content() {
    let fixture = fixture("error-before-content.json");
    assert!(fixture["content_at"].is_null());
    let swaps = fixture["swaps"].as_u64().unwrap();
    for sequence in fixture["sequences"].as_array().unwrap() {
        let mut hold = writer();
        let events = sequence.as_array().unwrap();
        for (index, event) in events.iter().enumerate() {
            hold.write_all(event.as_str().unwrap().as_bytes()).unwrap();
            if index + 1 == events.len() {
                assert_eq!(
                    u32::from(hold.failed_before_content()),
                    u32::try_from(swaps).unwrap()
                );
                assert!(!hold.committed());
                assert!(
                    hold.downstream().is_empty(),
                    "error before content reached the agent: {}",
                    String::from_utf8_lossy(hold.downstream())
                );
            } else {
                assert_eq!(u32::from(hold.failed_before_content()), 0);
                assert!(!hold.committed());
            }
        }
    }
}

#[test]
fn hold_keeps_upstream_after_first_content_byte() {
    let fixture = fixture("content-then-error.json");
    let events: Vec<&str> = fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event.as_str().unwrap())
        .collect();
    let content_at = fixture["content_at"].as_u64().unwrap() as usize;
    let mut hold = writer();

    hold.write_all(events[0].as_bytes()).unwrap();
    assert!(!hold.committed());

    let content = events[content_at];
    let cut = content.find("data:").unwrap();
    let content_bytes = content.as_bytes();
    hold.write_all(&content_bytes[..cut]).unwrap();
    assert!(
        !hold.committed(),
        "a content event split before its blank line is not content yet"
    );
    hold.write_all(&content_bytes[cut..]).unwrap();
    assert!(hold.committed());
    assert_eq!(u32::from(hold.failed_before_content()), 0);

    hold.write_all(events[2].as_bytes()).unwrap();
    assert_eq!(
        u32::from(hold.failed_before_content()),
        fixture["swaps"].as_u64().unwrap() as u32
    );
    assert!(hold.committed());
    let downstream = String::from_utf8_lossy(hold.downstream());
    for needle in fixture["downstream_has"].as_array().unwrap() {
        let needle = needle.as_str().unwrap();
        assert!(
            downstream.contains(needle),
            "downstream missing {needle}: {downstream}"
        );
    }
}

#[test]
fn hold_treats_lead_events_as_not_content() {
    let fixture = fixture("lead-events.json");
    let leads = fixture["leads"].as_array().unwrap();
    let content_at = fixture["content_at"].as_u64().unwrap() as usize;
    assert_eq!(leads.len(), content_at);

    let mut hold = writer();
    for lead in leads {
        hold.write_all(lead.as_str().unwrap().as_bytes()).unwrap();
        assert_eq!(u32::from(hold.failed_before_content()), 0);
        assert!(
            !hold.committed(),
            "lead counted as content: {}",
            lead.as_str().unwrap()
        );
    }
    hold.write_all(fixture["content"].as_str().unwrap().as_bytes())
        .unwrap();
    assert!(hold.committed());
    assert_eq!(
        u32::from(hold.failed_before_content()),
        fixture["swaps"].as_u64().unwrap() as u32
    );
    let downstream = String::from_utf8_lossy(hold.downstream());
    for needle in fixture["downstream_has"].as_array().unwrap() {
        assert!(downstream.contains(needle.as_str().unwrap()));
    }

    let mut done = writer();
    done.write_all(fixture["done"].as_str().unwrap().as_bytes())
        .unwrap();
    assert!(done.committed());
    assert!(!done.failed_before_content());

    let mut finished = writer();
    finished
        .write_all(fixture["finish"].as_str().unwrap().as_bytes())
        .unwrap();
    assert!(finished.committed());
    assert!(!finished.failed_before_content());
}

#[test]
fn hold_releases_at_15s_or_1mib() {
    let fixture = fixture("release.json");
    assert!(fixture["content_at"].is_null());
    let lead = fixture["lead"].as_str().unwrap();
    let hold_ms = fixture["hold_ms"].as_u64().unwrap();
    let hold_bytes = fixture["hold_bytes"].as_u64().unwrap() as usize;
    let error = fixture["error_after_release"].as_str().unwrap();

    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut hold = HoldWriter::new(Vec::new(), {
        let now = Rc::clone(&now);
        move || now.get()
    });
    hold.write_all(lead.as_bytes()).unwrap();
    now.set(Duration::from_millis(hold_ms));
    hold.write_all(lead.as_bytes()).unwrap();
    assert!(
        !hold.committed(),
        "exactly {hold_ms} ms is still inside the hold"
    );
    now.set(Duration::from_millis(hold_ms + 1));
    hold.write_all(lead.as_bytes()).unwrap();
    assert!(hold.committed());
    assert_eq!(u32::from(hold.failed_before_content()), 0);
    hold.write_all(error.as_bytes()).unwrap();
    assert_eq!(u32::from(hold.failed_before_content()), 0);
    assert!(String::from_utf8_lossy(hold.downstream()).contains("Overloaded"));

    let mut hold = writer();
    hold.write_all(&vec![b'x'; hold_bytes]).unwrap();
    assert!(
        !hold.committed(),
        "exactly {hold_bytes} bytes is still inside the hold"
    );
    hold.write_all(b"y").unwrap();
    assert!(hold.committed());
    assert_eq!(u32::from(hold.failed_before_content()), 0);
    assert_eq!(hold.downstream().len(), hold_bytes + 1);
    hold.write_all(error.as_bytes()).unwrap();
    assert_eq!(u32::from(hold.failed_before_content()), 0);
    assert!(hold.committed());

    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut late = HoldWriter::new(Vec::new(), {
        let now = Rc::clone(&now);
        move || now.get()
    });
    late.write_all(lead.as_bytes()).unwrap();
    now.set(Duration::from_millis(hold_ms + 1));
    late.write_all(error.as_bytes()).unwrap();
    assert!(late.failed_before_content());
    assert!(!late.committed());
    assert!(
        late.downstream().is_empty(),
        "an error after the time limit, with no content, still stays off the agent"
    );
}

fn writer() -> HoldWriter<Vec<u8>, impl Fn() -> Duration> {
    HoldWriter::new(Vec::new(), || Duration::ZERO)
}

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/hold")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"))
}
