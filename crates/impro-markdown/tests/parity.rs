//! Deterministic reference cases exercise combinations beyond the named suite.
use impro_markdown::{blockedit, mdview};
use serde_json::{json, Value};
#[test]
fn reference_combinations() {
    let fixtures: Value = serde_json::from_str(include_str!("fixtures/core.json")).unwrap();
    for pair in fixtures["blocks"].as_array().unwrap() {
        let text = pair[0].as_str().unwrap();
        let actual:Vec<_>=mdview::parse_blocks(text).map(|b|json!({"kind":b.kind,"text":b.text,"level":b.level,"indent":b.indent,"quote":b.quote,"marker":b.marker,"language":b.language,"cells":b.cells,"alignments":b.alignments,"header":b.header,"continuation":b.continuation})).collect();
        assert_eq!(json!(actual), pair[1], "blocks {text:?}");
    }
    for pair in fixtures["inline"].as_array().unwrap() {
        let text = pair[0].as_str().unwrap();
        let actual: Vec<_> = mdview::parse_inline(text)
            .into_iter()
            .map(|s| json!({"text":s.text,"styles":s.styles,"link":s.link}))
            .collect();
        assert_eq!(json!(actual), pair[1], "inline {text:?}");
    }
    for case in fixtures["edits"].as_array().unwrap() {
        let source = case[0].as_str().unwrap();
        let actual = blockedit::transform(
            source,
            case[1].as_u64().unwrap() as usize,
            case[2].as_str().unwrap(),
            case[3].as_u64().unwrap() as usize,
            case[4].as_bool().unwrap(),
        )
        .map(|e| json!({"start":e.start,"end":e.end,"text":e.text,"cursor":e.cursor}));
        assert_eq!(json!(actual), case[5], "edit {case}");
    }
}
