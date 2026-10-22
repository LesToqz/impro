use impro_markdown::{livetable::*, mdview};
const TABLE: &str = "| A | B |\n| --- | ---: |\n| 1 | 2 |\n| 3 | 4 |\n";
#[test]
fn test_cell_text_round_trip_escapes_pipes_and_breaks() {
    for (source, shown) in [
        ("a \\| b", "a | b"),
        ("one<br>two", "one\ntwo"),
        ("x<BR/>y", "x\ny"),
        ("`a\\|b`", "`a|b`"),
        ("\\\\\\|", "\\\\|"),
        ("plain", "plain"),
    ] {
        assert_eq!(to_display(source), shown);
    }
    assert_eq!(to_source("a | b\nc"), "a \\| b<br>c");
    assert_eq!(to_source("already \\| escaped"), "already \\| escaped");
    for text in ["a | b", "x\\\\|y", "|", "tail\\"] {
        assert_eq!(
            mdview::_cells(&serialize(&[to_source(text), "z".into()])).len(),
            2
        );
    }
}
#[test]
fn test_cell_spans_agree_with_mdview_cells() {
    for line in [
        "| a | b |",
        "a | b",
        "|  | x |",
        "| `c|d` | e \\| f |",
        "  | lead | trail |  ",
        "| only |",
        "| a | b | c",
        "|a|b|",
        " | 雪 | é ",
    ] {
        let chars: Vec<_> = line.chars().collect();
        let spans = cell_spans(line);
        let cells = mdview::_cells(line);
        assert_eq!(spans.len(), cells.len());
        for ((a, b), cell) in spans.into_iter().zip(cells) {
            assert_eq!(chars[a..b].iter().collect::<String>(), cell);
        }
    }
}
#[test]
fn test_model_round_trips_and_edits_keep_a_valid_table() {
    for text in [
        TABLE.into(),
        TABLE.trim_end_matches('\n').into(),
        TABLE.replace('\n', "\r\n"),
        "| A | B |\n| --- | --- |\n| ragged |\n| x | y | extra |\n".into(),
        "a | b\n--- | ---\n".into(),
    ] {
        assert_eq!(Table::new(&text).unwrap().text(), text);
    }
    let mut model = Table::new(TABLE).unwrap();
    model.set(1, 0, "new \\| cell");
    model.insert_column(1);
    model.set_align(2, "center");
    model.insert_row(3);
    model.delete_row(1);
    let text = model.text();
    assert_eq!(
        text,
        "| A |  | B |\n| --- | --- | :---: |\n| 3 |  | 4 |\n|  |  |  |\n"
    );
    let blocks: Vec<_> = mdview::parse_blocks(&text).collect();
    assert_eq!(blocks[0].cells, ["A", "", "B"]);
    assert_eq!(blocks[0].alignments, ["left", "left", "center"]);
    let mut header = Table::new(TABLE).unwrap();
    header.delete_row(2);
    header.delete_row(1);
    assert_eq!(header.text(), "| A | B |\n| --- | ---: |\n");
    assert_eq!(
        mdview::parse_blocks(&header.text()).next().unwrap().kind,
        "table"
    );
}
#[test]
fn test_markup_and_click_mapping() {
    assert_eq!(markup("**b** `c`<br>x"), "<b>b</b> <tt>c</tt>\nx");
    assert_eq!(align_index("bold x", "**bold** x", 0), 2);
    assert_eq!(align_index("bold x", "**bold** x", 5), 9);
}
#[test]
fn test_minimal_hunks_keep_unicode_final_newline_and_slash() {
    for (old, new) in [
        (TABLE, TABLE.replace("| 1 |", "| 1雪 |")),
        ("a\n", "a/\n".into()),
        ("a", "".into()),
        ("", "雪".into()),
        ("same", "same".into()),
    ] {
        let edit = minimal_edit(old, &new);
        assert_eq!(edit.apply(old), new);
        if old.ends_with('\n') && new.ends_with('\n') {
            assert!(edit.end < old.chars().count());
        }
        assert_ne!(edit.text, "/");
    }
}
