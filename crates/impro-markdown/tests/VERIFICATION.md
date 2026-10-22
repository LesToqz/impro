# Markdown integration verification

The executable tests cover pure behavior and GTK table widgets through a real
TextBuffer/TextView host. The renderer, slash menu and selection toolbar remain
owned by the next task. Their no-op modules were preserved. Do these checks with
the real editor/window once that task wires `TableHost`; a standalone host cannot
verify editor history, save behavior or replacement-block geometry.

From the block key suite, retain these names and assertions in that task:

- `test_typing_live_shortcuts_and_save`
- `test_active_prefixes_and_inline_span_visibility`
- `test_home_left_selection_copy_click_and_undo_clamp`
- `test_list_keys_children_renumber_and_one_step_undo`
- `test_split_demote_quote_exit_and_numbering_after_interruption`
- `test_code_keys_language_picker_copy_and_exit`
- `test_fence_caret_positions_stay_in_code_content`
- `test_divider_and_multiline_paste`

From the live table suite, verify the insertion flows:

- `test_slash_table_focuses_the_first_header_cell_with_text_selected`
- `test_toolbar_table_focuses_first_header_cell`

The other 18 table widget tests retain the source suite's names. Assertions
specific to saving, rebuilding the renderer's full block list and undo/redo
history need the real editor. The host tests verify begin/end action boundaries,
history forwarding and focus restoration, without implementing a second history
engine. The spacing test checks fresh/in-place grid height and absence of extra
line padding; the exact 181-pixel document extent remains a renderer check.
Native cell selection/copy uses the TextView signals, while grid navigation
exercises the same `TableView::key` method installed on key-press-event.

## Host wiring contract

1. Construct `Tables` lazily, passing `Rc<dyn TableHost>`. Keep the host's link to
   its renderer weak to avoid ownership cycles. All GTK calls stay on one thread.
2. Give each table block a stable id and `TableBlock` marks. Its offsets and all
   edit offsets count Unicode characters. Replacement reserves its estimated
   height and hides source before cells paint. `TableBlock::text()` changes on
   every silent write, before `retag` and `queue_layout` run.
3. `silent(true/false)` blocks only the renderer's insert/delete/changed and caret
   handlers. Keep editor history observing the underlying buffer signals.
   `retag` restores the leading replacement tag, invisible body and gap tags.
4. Drive `fill_visible_slice(viewport_rows)` first, yielding between cells, and
   `fill_slice()` for remaining rows if desired. No timer is installed for grid
   construction. Spacers reserve unbuilt rows. Caret entry materializes its row.
5. Call `caret(Some(block_id), offset)` for a table: true means its source must
   remain hidden. Call `caret(None, offset)` outside tables to remember direction.
   Forward undo/redo through `history(redo)`; call `remember` before disposing a
   focused grid, then rebuild from the newly restored source. Pending focus
   expires after three seconds. Selection is passed to `focus_cell` explicitly.
6. `remove(id)` immediately destroys disappeared grids and ends the typing burst.
   `destroy()` closes all tables before renderer shutdown. The host deletes its
   block marks with `TableBlock::delete_marks` when discarding the block.
7. Call crate `init()` before theme application. The registered table CSS follows
   shared palette changes. Use `styled_css` in the renderer's body-font provider;
   pass its effective foreground through the `foreground` palette role.

## Reproduce standalone visual comparison

Build `cargo build -p impro-markdown --example table_demo`. Run its binary with
`impro-dark .ceo/table-dark.png` and `impro-light .ceo/table-light.png` under
`GDK_BACKEND=x11 xvfb-run -a dbus-run-session --`. The optional path saves a PNG
and exits; without it the example is interactive. The local comparison harness
is `.ceo/table_qa.py`; give it the read-only reference root, mode and PNG path.
The default sample covers alignment, inline styles, line breaks and escaped pipes.
