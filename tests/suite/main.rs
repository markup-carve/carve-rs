//! Every integration test except the few below, compiled as ONE binary.
//!
//! One file per binary meant ~750 crates to compile and link on every CI run.
//! Kept out, because CI addresses them by target name: `perf_regressions`
//! (guards.yml), `includes` and `include_conformance` (the no-fs job).
//!
//! A new `tests/*.rs` needs a line here; `registry::every_test_file_is_compiled`
//! fails until it has one.

#[path = "../common/mod.rs"]
mod common;

#[path = "../a_band_line_under_a_block_that_interrupted_the_lead_paragraph.rs"]
mod a_band_line_under_a_block_that_interrupted_the_lead_paragraph;
#[path = "../a_band_line_under_a_marker_line_block_ends_the_item.rs"]
mod a_band_line_under_a_marker_line_block_ends_the_item;
#[path = "../a_band_line_under_an_empty_marker_line_quote_ends_the_item.rs"]
mod a_band_line_under_an_empty_marker_line_quote_ends_the_item;
#[path = "../a_band_paragraph_after_an_invisible_line_loosens_its_item.rs"]
mod a_band_paragraph_after_an_invisible_line_loosens_its_item;
#[path = "../a_bare_closer_does_not_reach_inside_a_link_destination.rs"]
mod a_bare_closer_does_not_reach_inside_a_link_destination;
#[path = "../a_bare_empty_paragraph_leaves_both_exits.rs"]
mod a_bare_empty_paragraph_leaves_both_exits;
#[path = "../a_bare_marker_does_not_close_inside_a_braced_inline.rs"]
mod a_bare_marker_does_not_close_inside_a_braced_inline;
#[path = "../a_bare_opener_the_writer_would_refuse_takes_braces.rs"]
mod a_bare_opener_the_writer_would_refuse_takes_braces;
#[path = "../a_below_column_colon_fence_run_folds.rs"]
mod a_below_column_colon_fence_run_folds;
#[path = "../a_below_column_comment_ends_no_item.rs"]
mod a_below_column_comment_ends_no_item;
#[path = "../a_below_column_marker_after_a_lazy_line_folds.rs"]
mod a_below_column_marker_after_a_lazy_line_folds;
#[path = "../a_blank_inside_a_sibling_sub_list_does_not_loosen_the_outer_list.rs"]
mod a_blank_inside_a_sibling_sub_list_does_not_loosen_the_outer_list;
#[path = "../a_blank_inside_an_unopened_fence_span_is_interior.rs"]
mod a_blank_inside_an_unopened_fence_span_is_interior;
#[path = "../a_blank_line_does_not_end_a_definition_s_authored_base.rs"]
mod a_blank_line_does_not_end_a_definition_s_authored_base;
#[path = "../a_blank_line_does_not_end_a_list_s_authored_base.rs"]
mod a_blank_line_does_not_end_a_list_s_authored_base;
#[path = "../a_blank_line_loosens_an_item_only_when_a_paragraph_follows_it.rs"]
mod a_blank_line_loosens_an_item_only_when_a_paragraph_follows_it;
#[path = "../a_block_cell_image_writes_as_an_image.rs"]
mod a_block_cell_image_writes_as_an_image;
#[path = "../a_block_extension_declares_its_fallback.rs"]
mod a_block_extension_declares_its_fallback;
#[path = "../a_block_image_and_a_merged_run_in_a_tight_item_keep_their_item.rs"]
mod a_block_image_and_a_merged_run_in_a_tight_item_keep_their_item;
#[path = "../a_block_opener_below_a_block_at_the_minimum_column.rs"]
mod a_block_opener_below_a_block_at_the_minimum_column;
#[path = "../a_block_opener_between_two_content_columns_opens.rs"]
mod a_block_opener_between_two_content_columns_opens;
#[path = "../a_boolean_attribute_does_not_start_with_an_underscore.rs"]
mod a_boolean_attribute_does_not_start_with_an_underscore;
#[path = "../a_boundary_line_inside_an_open_fence.rs"]
mod a_boundary_line_inside_an_open_fence;
#[path = "../a_braced_span_in_a_braced_span_of_its_kind_is_unspellable.rs"]
mod a_braced_span_in_a_braced_span_of_its_kind_is_unspellable;
#[path = "../a_bullet_task_item_reads_its_box.rs"]
mod a_bullet_task_item_reads_its_box;
#[path = "../a_captioned_fence_keeps_its_renderer.rs"]
mod a_captioned_fence_keeps_its_renderer;
#[path = "../a_caret_before_a_bracket_node_keeps_its_escape.rs"]
mod a_caret_before_a_bracket_node_keeps_its_escape;
#[path = "../a_caret_before_a_bracket_that_opens_no_note.rs"]
mod a_caret_before_a_bracket_that_opens_no_note;
#[path = "../a_carried_run_keeps_its_width_across_the_row_boundary.rs"]
mod a_carried_run_keeps_its_width_across_the_row_boundary;
#[path = "../a_cell_alignment_imports_as_the_native_marker.rs"]
mod a_cell_alignment_imports_as_the_native_marker;
#[path = "../a_citation_definition_is_a_node.rs"]
mod a_citation_definition_is_a_node;
#[path = "../a_citation_item_carries_its_own_mode.rs"]
mod a_citation_item_carries_its_own_mode;
#[path = "../a_class_key_value_is_the_class_slot.rs"]
mod a_class_key_value_is_the_class_slot;
#[path = "../a_closed_comment_span_ends_the_right_item.rs"]
mod a_closed_comment_span_ends_the_right_item;
#[path = "../a_code_blocks_last_newline_is_its_terminator.rs"]
mod a_code_blocks_last_newline_is_its_terminator;
#[path = "../a_code_blocks_nbsp_sentinel_is_resolved.rs"]
mod a_code_blocks_nbsp_sentinel_is_resolved;
#[path = "../a_code_span_closer_is_searched_across_the_block.rs"]
mod a_code_span_closer_is_searched_across_the_block;
#[path = "../a_colon_container_keeps_a_band_payload_at_its_column.rs"]
mod a_colon_container_keeps_a_band_payload_at_its_column;
#[path = "../a_colon_fence_container_owns_a_link_def_past_its_column.rs"]
mod a_colon_fence_container_owns_a_link_def_past_its_column;
#[path = "../a_colon_followed_by_only_whitespace_is_not_a_description.rs"]
mod a_colon_followed_by_only_whitespace_is_not_a_description;
#[path = "../a_colon_name_before_a_bracket_node_keeps_its_escape.rs"]
mod a_colon_name_before_a_bracket_node_keeps_its_escape;
#[path = "../a_combined_closer_carries_its_attribute_block.rs"]
mod a_combined_closer_carries_its_attribute_block;
#[path = "../a_comment_at_the_minimum_column_is_not_ownership_evidence.rs"]
mod a_comment_at_the_minimum_column_is_not_ownership_evidence;
#[path = "../a_comment_below_a_sub_list_in_a_tight_item_keeps_its_column.rs"]
mod a_comment_below_a_sub_list_in_a_tight_item_keeps_its_column;
#[path = "../a_comment_in_a_footnote_body_leaves_no_blank_line.rs"]
mod a_comment_in_a_footnote_body_leaves_no_blank_line;
#[path = "../a_comment_keeps_the_space_that_separates_it.rs"]
mod a_comment_keeps_the_space_that_separates_it;
#[path = "../a_comment_leaf_begins_at_its_markup.rs"]
mod a_comment_leaf_begins_at_its_markup;
#[path = "../a_comment_only_verse_line_is_removed_at_the_block_layer.rs"]
mod a_comment_only_verse_line_is_removed_at_the_block_layer;
#[path = "../a_comment_span_closer_below_the_column_is_still_its_closer.rs"]
mod a_comment_span_closer_below_the_column_is_still_its_closer;
#[path = "../a_const_valued_wire_field_is_checked_at_decode.rs"]
mod a_const_valued_wire_field_is_checked_at_decode;
#[path = "../a_consumed_caption_reports_what_it_dropped.rs"]
mod a_consumed_caption_reports_what_it_dropped;
#[path = "../a_contained_comment_is_measured_by_the_column_it_reaches.rs"]
mod a_contained_comment_is_measured_by_the_column_it_reaches;
#[path = "../a_container_body_is_parsed_off_the_stack.rs"]
mod a_container_body_is_parsed_off_the_stack;
#[path = "../a_container_closer_is_not_absorbable.rs"]
mod a_container_closer_is_not_absorbable;
#[path = "../a_container_ends_at_its_last_placed_child.rs"]
mod a_container_ends_at_its_last_placed_child;
#[path = "../a_container_ends_at_the_markup_that_closes_it.rs"]
mod a_container_ends_at_the_markup_that_closes_it;
#[path = "../a_container_ends_where_its_last_block_leaves_no_paragraph_open.rs"]
mod a_container_ends_where_its_last_block_leaves_no_paragraph_open;
#[path = "../a_container_label_survives_an_html_import.rs"]
mod a_container_label_survives_an_html_import;
#[path = "../a_container_starts_at_its_opening_markup.rs"]
mod a_container_starts_at_its_opening_markup;
#[path = "../a_container_stops_at_the_definition_it_hosted.rs"]
mod a_container_stops_at_the_definition_it_hosted;
#[path = "../a_content_space_in_the_raw_dom_is_not_blank.rs"]
mod a_content_space_in_the_raw_dom_is_not_blank;
#[path = "../a_continuation_marker_attaches_one_block.rs"]
mod a_continuation_marker_attaches_one_block;
#[path = "../a_continuation_marker_attaches_only_a_flush_left_block.rs"]
mod a_continuation_marker_attaches_only_a_flush_left_block;
#[path = "../a_continuation_rows_open_run_spans_it.rs"]
mod a_continuation_rows_open_run_spans_it;
#[path = "../a_crossref_label_built_from_an_expansion_degrades_to_its_target.rs"]
mod a_crossref_label_built_from_an_expansion_degrades_to_its_target;
#[path = "../a_dd_hosted_note_absorbs_an_opener_at_its_floor.rs"]
mod a_dd_hosted_note_absorbs_an_opener_at_its_floor;
#[path = "../a_declining_extension_keeps_the_authored_attributes.rs"]
mod a_declining_extension_keeps_the_authored_attributes;
#[path = "../a_definition_behind_an_alternating_container_prefix_registers_at_every_depth.rs"]
mod a_definition_behind_an_alternating_container_prefix_registers_at_every_depth;
#[path = "../a_definition_behind_an_alternating_prefix_registers.rs"]
mod a_definition_behind_an_alternating_prefix_registers;
#[path = "../a_definition_between_two_content_columns_registers.rs"]
mod a_definition_between_two_content_columns_registers;
#[path = "../a_definition_body_s_separator_width_sets_its_content_column.rs"]
mod a_definition_body_s_separator_width_sets_its_content_column;
#[path = "../a_definition_body_with_no_open_paragraph_does_not_fold.rs"]
mod a_definition_body_with_no_open_paragraph_does_not_fold;
#[path = "../a_definition_description_ends_at_its_last_placed_child.rs"]
mod a_definition_description_ends_at_its_last_placed_child;
#[path = "../a_definition_destination_is_a_link_destination.rs"]
mod a_definition_destination_is_a_link_destination;
#[path = "../a_definition_in_a_contained_comment_registers_nothing.rs"]
mod a_definition_in_a_contained_comment_registers_nothing;
#[path = "../a_definition_in_a_description_inside_a_container.rs"]
mod a_definition_in_a_description_inside_a_container;
#[path = "../a_definition_in_a_quoted_comment_registers_nothing.rs"]
mod a_definition_in_a_quoted_comment_registers_nothing;
#[path = "../a_definition_in_an_attached_block_is_the_blocks.rs"]
mod a_definition_in_an_attached_block_is_the_blocks;
#[path = "../a_definition_list_ends_at_its_last_placed_child.rs"]
mod a_definition_list_ends_at_its_last_placed_child;
#[path = "../a_definition_marker_line_answers_s4_like_a_list_marker.rs"]
mod a_definition_marker_line_answers_s4_like_a_list_marker;
#[path = "../a_definition_on_a_lazy_continuation_line_is_not_collected.rs"]
mod a_definition_on_a_lazy_continuation_line_is_not_collected;
#[path = "../a_definition_terms_continuation_line_drops_its_trailing_run.rs"]
mod a_definition_terms_continuation_line_drops_its_trailing_run;
#[path = "../a_degraded_comment_fence_at_a_containers_column_zero.rs"]
mod a_degraded_comment_fence_at_a_containers_column_zero;
#[path = "../a_degraded_comment_fence_at_a_descendants_column.rs"]
mod a_degraded_comment_fence_at_a_descendants_column;
#[path = "../a_degraded_comment_fence_at_a_non_descendant_column.rs"]
mod a_degraded_comment_fence_at_a_non_descendant_column;
#[path = "../a_degraded_comment_fence_leaves_a_lazy_follower.rs"]
mod a_degraded_comment_fence_leaves_a_lazy_follower;
#[path = "../a_deletion_survives_an_html_import.rs"]
mod a_deletion_survives_an_html_import;
#[path = "../a_delimited_comment_whose_text_opens_with_a_break_takes_no_pad.rs"]
mod a_delimited_comment_whose_text_opens_with_a_break_takes_no_pad;
#[path = "../a_denied_scheme_split_by_del_does_not_reach_the_href.rs"]
mod a_denied_scheme_split_by_del_does_not_reach_the_href;
#[path = "../a_derived_accessible_name_does_not_come_back.rs"]
mod a_derived_accessible_name_does_not_come_back;
#[path = "../a_derived_label_keeps_an_escaped_character.rs"]
mod a_derived_label_keeps_an_escaped_character;
#[path = "../a_derived_title_id_survives_a_namespace_collision.rs"]
mod a_derived_title_id_survives_a_namespace_collision;
#[path = "../a_description_body_definition_registers_across_a_blank.rs"]
mod a_description_body_definition_registers_across_a_blank;
#[path = "../a_description_body_under_a_quoted_item_is_the_body.rs"]
mod a_description_body_under_a_quoted_item_is_the_body;
#[path = "../a_description_bodys_code_block_is_placed.rs"]
mod a_description_bodys_code_block_is_placed;
#[path = "../a_description_hosted_list_extent_starts_at_its_marker.rs"]
mod a_description_hosted_list_extent_starts_at_its_marker;
#[path = "../a_description_item_after_a_nested_closed_fence_is_its_own_item.rs"]
mod a_description_item_after_a_nested_closed_fence_is_its_own_item;
#[path = "../a_destination_less_element_keeps_its_title.rs"]
mod a_destination_less_element_keeps_its_title;
#[path = "../a_directive_title_is_walked_like_an_admonitions.rs"]
mod a_directive_title_is_walked_like_an_admonitions;
#[path = "../a_display_equation_carries_its_label_and_number.rs"]
mod a_display_equation_carries_its_label_and_number;
#[path = "../a_document_level_rebuild_keeps_a_trailing_blank.rs"]
mod a_document_level_rebuild_keeps_a_trailing_blank;
#[path = "../a_fence_in_a_nested_quote_leaves_no_lazy_claim.rs"]
mod a_fence_in_a_nested_quote_leaves_no_lazy_claim;
#[path = "../a_fence_opener_drops_its_trailing_tab.rs"]
mod a_fence_opener_drops_its_trailing_tab;
#[path = "../a_fenced_block_quote.rs"]
mod a_fenced_block_quote;
#[path = "../a_fenced_block_quote_caption.rs"]
mod a_fenced_block_quote_caption;
#[path = "../a_figcaption_holding_a_content_space_keeps_its_figure.rs"]
mod a_figcaption_holding_a_content_space_keeps_its_figure;
#[path = "../a_figure_around_a_self_captioning_table.rs"]
mod a_figure_around_a_self_captioning_table;
#[path = "../a_figure_group_degrades_deterministically.rs"]
mod a_figure_group_degrades_deterministically;
#[path = "../a_figure_group_imports_its_own_html.rs"]
mod a_figure_group_imports_its_own_html;
#[path = "../a_figure_group_is_one_numbering_unit.rs"]
mod a_figure_group_is_one_numbering_unit;
#[path = "../a_figure_group_serializes_as_its_own_type.rs"]
mod a_figure_group_serializes_as_its_own_type;
#[path = "../a_figure_group_writes_back_as_authored.rs"]
mod a_figure_group_writes_back_as_authored;
#[path = "../a_figure_spelling_that_is_not_a_group_lints.rs"]
mod a_figure_spelling_that_is_not_a_group_lints;
#[path = "../a_figure_unwraps_when_its_target_cannot_carry_a_caption.rs"]
mod a_figure_unwraps_when_its_target_cannot_carry_a_caption;
#[path = "../a_figures_target_keeps_its_own_attributes.rs"]
mod a_figures_target_keeps_its_own_attributes;
#[path = "../a_flag_shaped_hyphen_run_is_literal.rs"]
mod a_flag_shaped_hyphen_run_is_literal;
#[path = "../a_flatten_preserves_the_boundary_it_dissolves.rs"]
mod a_flatten_preserves_the_boundary_it_dissolves;
#[path = "../a_flush_left_opener_ends_a_description_body.rs"]
mod a_flush_left_opener_ends_a_description_body;
#[path = "../a_folded_lazy_line_leaves_the_container_open.rs"]
mod a_folded_lazy_line_leaves_the_container_open;
#[path = "../a_folded_term_s_part_boundary_break_is_placed.rs"]
mod a_folded_term_s_part_boundary_break_is_placed;
#[path = "../a_footnote_backlink_says_where_it_goes.rs"]
mod a_footnote_backlink_says_where_it_goes;
#[path = "../a_footnote_chrome_test_reads_a_content_space_as_content.rs"]
mod a_footnote_chrome_test_reads_a_content_space_as_content;
#[path = "../a_footnote_continuation_survives_a_blank_run.rs"]
mod a_footnote_continuation_survives_a_blank_run;
#[path = "../a_footnote_nested_past_a_footnote_body_is_its_own_note.rs"]
mod a_footnote_nested_past_a_footnote_body_is_its_own_note;
#[path = "../a_foreign_figure_imports_as_a_caption_line.rs"]
mod a_foreign_figure_imports_as_a_caption_line;
#[path = "../a_form_feed_is_content_in_the_bold_italic_guard.rs"]
mod a_form_feed_is_content_in_the_bold_italic_guard;
#[path = "../a_further_plus_ends_the_attached_block.rs"]
mod a_further_plus_ends_the_attached_block;
#[path = "../a_generated_content_kind_parses_to_a_directive.rs"]
mod a_generated_content_kind_parses_to_a_directive;
#[path = "../a_hard_break_in_a_table_cell_is_one_space.rs"]
mod a_hard_break_in_a_table_cell_is_one_space;
#[path = "../a_headings_math_feeds_its_id.rs"]
mod a_headings_math_feeds_its_id;
#[path = "../a_headings_trailing_hash_run_is_escaped.rs"]
mod a_headings_trailing_hash_run_is_escaped;
#[path = "../a_hoisted_definition_stays_on_an_emptied_item_marker.rs"]
mod a_hoisted_definition_stays_on_an_emptied_item_marker;
#[path = "../a_hoisted_section_id_comes_back_on_its_heading.rs"]
mod a_hoisted_section_id_comes_back_on_its_heading;
#[path = "../a_line_block_hardens_at_every_depth.rs"]
mod a_line_block_hardens_at_every_depth;
#[path = "../a_line_block_round_trips_every_three_line_shape.rs"]
mod a_line_block_round_trips_every_three_line_shape;
#[path = "../a_line_comment_drops_trailing_whitespace.rs"]
mod a_line_comment_drops_trailing_whitespace;
#[path = "../a_line_initial_code_payload_keeps_its_padding.rs"]
mod a_line_initial_code_payload_keeps_its_padding;
#[path = "../a_link_policy_reads_the_host_a_browser_reads.rs"]
mod a_link_policy_reads_the_host_a_browser_reads;
#[path = "../a_lint_diagnostic_reads_the_line_it_reports_on.rs"]
mod a_lint_diagnostic_reads_the_line_it_reports_on;
#[path = "../a_list_marker_in_a_contained_comment_is_comment_text.rs"]
mod a_list_marker_in_a_contained_comment_is_comment_text;
#[path = "../a_literal_tilde_in_text_is_escaped.rs"]
mod a_literal_tilde_in_text_is_escaped;
#[path = "../a_lone_bracket_is_escaped_in_the_minimal_form.rs"]
mod a_lone_bracket_is_escaped_in_the_minimal_form;
#[path = "../a_lone_image_at_an_items_content_column_keeps_the_items_looseness.rs"]
mod a_lone_image_at_an_items_content_column_keeps_the_items_looseness;
#[path = "../a_lone_image_is_a_block_not_a_synthesized_paragraph.rs"]
mod a_lone_image_is_a_block_not_a_synthesized_paragraph;
#[path = "../a_lone_pipe_line_does_not_panic.rs"]
mod a_lone_pipe_line_does_not_panic;
#[path = "../a_markdown_link_with_an_empty_destination_is_its_text.rs"]
mod a_markdown_link_with_an_empty_destination_is_its_text;
#[path = "../a_markdown_run_that_cannot_flank_falls_back.rs"]
mod a_markdown_run_that_cannot_flank_falls_back;
#[path = "../a_marker_at_a_content_column_opens_a_sublist.rs"]
mod a_marker_at_a_content_column_opens_a_sublist;
#[path = "../a_marker_left_of_a_quotes_column_is_text.rs"]
mod a_marker_left_of_a_quotes_column_is_text;
#[path = "../a_marker_line_block_that_leaves_no_paragraph_ends_the_item.rs"]
mod a_marker_line_block_that_leaves_no_paragraph_ends_the_item;
#[path = "../a_marker_line_link_definition_is_collected_where_no_paragraph_is_open.rs"]
mod a_marker_line_link_definition_is_collected_where_no_paragraph_is_open;
#[path = "../a_math_span_survives_an_html_import.rs"]
mod a_math_span_survives_an_html_import;
#[path = "../a_media_fallback_converts_as_blocks.rs"]
mod a_media_fallback_converts_as_blocks;
#[path = "../a_mention_against_a_word_character_is_unspellable.rs"]
mod a_mention_against_a_word_character_is_unspellable;
#[path = "../a_mention_name_the_grammar_rejects_is_unspellable.rs"]
mod a_mention_name_the_grammar_rejects_is_unspellable;
#[path = "../a_multi_line_comment_in_a_table_cell_is_dropped.rs"]
mod a_multi_line_comment_in_a_table_cell_is_dropped;
#[path = "../a_nested_emphasis_keeps_its_nesting_in_markdown.rs"]
mod a_nested_emphasis_keeps_its_nesting_in_markdown;
#[path = "../a_nested_item_lead_fence_or_container_owns_its_flush_left_body.rs"]
mod a_nested_item_lead_fence_or_container_owns_its_flush_left_body;
#[path = "../a_nested_link_and_an_autolink_stay_nodes.rs"]
mod a_nested_link_and_an_autolink_stay_nodes;
#[path = "../a_nested_note_definition_is_at_the_bodys_own_column.rs"]
mod a_nested_note_definition_is_at_the_bodys_own_column;
#[path = "../a_nested_verse_break_owns_only_the_boundary.rs"]
mod a_nested_verse_break_owns_only_the_boundary;
#[path = "../a_nested_verse_comment_keeps_its_text.rs"]
mod a_nested_verse_comment_keeps_its_text;
#[path = "../a_new_marker_does_not_reach_a_dead_container_column.rs"]
mod a_new_marker_does_not_reach_a_dead_container_column;
#[path = "../a_non_li_child_of_a_list_is_reported_and_kept.rs"]
mod a_non_li_child_of_a_list_is_reported_and_kept;
#[path = "../a_note_ends_at_a_definition_it_cannot_take.rs"]
mod a_note_ends_at_a_definition_it_cannot_take;
#[path = "../a_note_in_an_unresolved_reference_is_not_a_reference.rs"]
mod a_note_in_an_unresolved_reference_is_not_a_reference;
#[path = "../a_panel_caption_number_stays_literal.rs"]
mod a_panel_caption_number_stays_literal;
#[path = "../a_paragraph_opened_after_a_block_in_an_item_is_still_open.rs"]
mod a_paragraph_opened_after_a_block_in_an_item_is_still_open;
#[path = "../a_partly_consumed_container_tab_keeps_the_authored_span.rs"]
mod a_partly_consumed_container_tab_keeps_the_authored_span;
#[path = "../a_patch_value_keeps_its_number_form.rs"]
mod a_patch_value_keeps_its_number_form;
#[path = "../a_percent_leading_inline_comment_joins_its_opener.rs"]
mod a_percent_leading_inline_comment_joins_its_opener;
#[path = "../a_placed_element_writes_one_column.rs"]
mod a_placed_element_writes_one_column;
#[path = "../a_placement_marker_places_only_at_document_top_level.rs"]
mod a_placement_marker_places_only_at_document_top_level;
#[path = "../a_plus_attached_comment_is_a_block_comment.rs"]
mod a_plus_attached_comment_is_a_block_comment;
#[path = "../a_preserved_element_does_not_report_its_attributes_dropped.rs"]
mod a_preserved_element_does_not_report_its_attributes_dropped;
#[path = "../a_profile_keeps_a_bodyless_named_container.rs"]
mod a_profile_keeps_a_bodyless_named_container;
#[path = "../a_profile_violation_refuses_instead_of_rendering_nothing.rs"]
mod a_profile_violation_refuses_instead_of_rendering_nothing;
#[path = "../a_quote_fence_below_a_quote_lints.rs"]
mod a_quote_fence_below_a_quote_lints;
#[path = "../a_quoted_lines_indent_is_not_content.rs"]
mod a_quoted_lines_indent_is_not_content;
#[path = "../a_quoted_marker_line_keeps_the_definitions_text.rs"]
mod a_quoted_marker_line_keeps_the_definitions_text;
#[path = "../a_quoted_title_holds_its_backslash.rs"]
mod a_quoted_title_holds_its_backslash;
#[path = "../a_raw_bracketed_run_is_written_as_authored.rs"]
mod a_raw_bracketed_run_is_written_as_authored;
#[path = "../a_raw_kept_element_reports_its_descendants_attributes.rs"]
mod a_raw_kept_element_reports_its_descendants_attributes;
#[path = "../a_reference_inside_an_inline_note_resolves.rs"]
mod a_reference_inside_an_inline_note_resolves;
#[path = "../a_referenced_abbreviation_definition_splits_by_target.rs"]
mod a_referenced_abbreviation_definition_splits_by_target;
#[path = "../a_refused_declaration_in_style_is_a_refused_attribute.rs"]
mod a_refused_declaration_in_style_is_a_refused_attribute;
#[path = "../a_report_is_ordered_by_the_losing_elements_position.rs"]
mod a_report_is_ordered_by_the_losing_elements_position;
#[path = "../a_roundtrip_figure_rebuilds_only_where_a_carve_spelling_reproduces_it.rs"]
mod a_roundtrip_figure_rebuilds_only_where_a_carve_spelling_reproduces_it;
#[path = "../a_row_of_blank_cells_is_unspellable.rs"]
mod a_row_of_blank_cells_is_unspellable;
#[path = "../a_row_splitter_carries_a_multi_backtick_run.rs"]
mod a_row_splitter_carries_a_multi_backtick_run;
#[path = "../a_sliced_include_keeps_its_own_file_s_coordinates.rs"]
mod a_sliced_include_keeps_its_own_file_s_coordinates;
#[path = "../a_small_caps_span_survives_interchange.rs"]
mod a_small_caps_span_survives_interchange;
#[path = "../a_span_contributes_its_text_to_a_heading_id.rs"]
mod a_span_contributes_its_text_to_a_heading_id;
#[path = "../a_span_label_that_opens_a_note_reference_is_escaped.rs"]
mod a_span_label_that_opens_a_note_reference_is_escaped;
#[path = "../a_split_scheme_does_not_defeat_a_link_policy.rs"]
mod a_split_scheme_does_not_defeat_a_link_policy;
#[path = "../a_structural_attribute_leads_the_authors_own.rs"]
mod a_structural_attribute_leads_the_authors_own;
#[path = "../a_structural_base_class_merges_whole_author_entries.rs"]
mod a_structural_base_class_merges_whole_author_entries;
#[path = "../a_structural_class_precedes_an_engine_minted_name.rs"]
mod a_structural_class_precedes_an_engine_minted_name;
#[path = "../a_substitutions_halves_are_inline_content.rs"]
mod a_substitutions_halves_are_inline_content;
#[path = "../a_tab_against_a_bare_delimiter_is_whitespace.rs"]
mod a_tab_against_a_bare_delimiter_is_whitespace;
#[path = "../a_tab_and_four_spaces_are_the_same_column.rs"]
mod a_tab_and_four_spaces_are_the_same_column;
#[path = "../a_tab_control_is_a_button_and_one_item_is_selected.rs"]
mod a_tab_control_is_a_button_and_one_item_is_selected;
#[path = "../a_table_caption_is_numbered_among_the_tables_children.rs"]
mod a_table_caption_is_numbered_among_the_tables_children;
#[path = "../a_table_cells_blocks_contribute_only_their_payload.rs"]
mod a_table_cells_blocks_contribute_only_their_payload;
#[path = "../a_table_section_keeps_the_attributes_it_has_a_slot_for.rs"]
mod a_table_section_keeps_the_attributes_it_has_a_slot_for;
#[path = "../a_table_whose_cells_hold_blocks_imports_as_a_list_table.rs"]
mod a_table_whose_cells_hold_blocks_imports_as_a_list_table;
#[path = "../a_task_box_is_content_not_a_prefix.rs"]
mod a_task_box_is_content_not_a_prefix;
#[path = "../a_task_box_is_read_only_where_the_extension_reaches.rs"]
mod a_task_box_is_read_only_where_the_extension_reaches;
#[path = "../a_task_items_checkbox_is_not_decided_by_its_first_block.rs"]
mod a_task_items_checkbox_is_not_decided_by_its_first_block;
#[path = "../a_task_items_checkbox_is_not_part_of_its_marker.rs"]
mod a_task_items_checkbox_is_not_part_of_its_marker;
#[path = "../a_task_state_names_itself_in_html.rs"]
mod a_task_state_names_itself_in_html;
#[path = "../a_task_state_survives_a_format_cycle.rs"]
mod a_task_state_survives_a_format_cycle;
#[path = "../a_term_folds_an_indented_block_opener_at_every_depth.rs"]
mod a_term_folds_an_indented_block_opener_at_every_depth;
#[path = "../a_titles_words_survive_the_editor_flatten.rs"]
mod a_titles_words_survive_the_editor_flatten;
#[path = "../a_trailing_header_span_writes_the_native_form.rs"]
mod a_trailing_header_span_writes_the_native_form;
#[path = "../a_trailing_line_in_a_nested_note_is_placed_by_column_reach.rs"]
mod a_trailing_line_in_a_nested_note_is_placed_by_column_reach;
#[path = "../a_trailing_tab_on_a_content_less_delimiter.rs"]
mod a_trailing_tab_on_a_content_less_delimiter;
#[path = "../a_url_list_attribute_is_probed_at_every_candidate.rs"]
mod a_url_list_attribute_is_probed_at_every_candidate;
#[path = "../a_used_up_backtick_leaves_later_brackets_alone.rs"]
mod a_used_up_backtick_leaves_later_brackets_alone;
#[path = "../a_verse_hard_break_keeps_its_backslash.rs"]
mod a_verse_hard_break_keeps_its_backslash;
#[path = "../a_whitespace_only_block_keeps_the_hard_list_boundary.rs"]
mod a_whitespace_only_block_keeps_the_hard_list_boundary;
#[path = "../a_whitespace_only_code_line_in_an_item_keeps_its_residue.rs"]
mod a_whitespace_only_code_line_in_an_item_keeps_its_residue;
#[path = "../a_whitespace_only_verbatim_line_uses_the_fence_column.rs"]
mod a_whitespace_only_verbatim_line_uses_the_fence_column;
#[path = "../a_word_processor_footnote_imports_as_a_footnote.rs"]
mod a_word_processor_footnote_imports_as_a_footnote;
#[path = "../a_wrapped_attribute_block_ends_a_quote_like_the_single_line_one.rs"]
mod a_wrapped_attribute_block_ends_a_quote_like_the_single_line_one;
#[path = "../a_wrapped_attribute_block_interrupts_a_paragraph.rs"]
mod a_wrapped_attribute_block_interrupts_a_paragraph;
#[path = "../abbreviation_def_document_level.rs"]
mod abbreviation_def_document_level;
#[path = "../abbreviation_expansion_required.rs"]
mod abbreviation_expansion_required;
#[path = "../abbreviation_inside_a_container.rs"]
mod abbreviation_inside_a_container;
#[path = "../abbreviation_security.rs"]
mod abbreviation_security;
#[path = "../abbreviation_split_positions.rs"]
mod abbreviation_split_positions;
#[path = "../abbreviation_term_is_ascii.rs"]
mod abbreviation_term_is_ascii;
#[path = "../absorbed_colon_fence_in_a_quote.rs"]
mod absorbed_colon_fence_in_a_quote;
#[path = "../absorbed_colon_fence_in_an_item.rs"]
mod absorbed_colon_fence_in_an_item;
#[path = "../accessibility_lint.rs"]
mod accessibility_lint;
#[path = "../accessible_names_tier3.rs"]
mod accessible_names_tier3;
#[path = "../adjacent_attached_block_openers.rs"]
mod adjacent_attached_block_openers;
#[path = "../adjacent_definition_lists_import_as_one.rs"]
mod adjacent_definition_lists_import_as_one;
#[path = "../adjacent_markdown_delimiter_runs_do_not_merge.rs"]
mod adjacent_markdown_delimiter_runs_do_not_merge;
#[path = "../adjacent_sibling_lists_stay_separate.rs"]
mod adjacent_sibling_lists_stay_separate;
#[path = "../adjacent_text_is_written_as_one_run.rs"]
mod adjacent_text_is_written_as_one_run;
#[path = "../adjacent_text_runs.rs"]
mod adjacent_text_runs;
#[path = "../admonition_tier1_trust_class.rs"]
mod admonition_tier1_trust_class;
#[path = "../admonition_title_before_label.rs"]
mod admonition_title_before_label;
#[path = "../an_aligned_text_block_renders_a_style_declaration.rs"]
mod an_aligned_text_block_renders_a_style_declaration;
#[path = "../an_all_blank_table_row_is_not_a_table.rs"]
mod an_all_blank_table_row_is_not_a_table;
#[path = "../an_attribute_dropped_row_counts_attributes_not_class_names.rs"]
mod an_attribute_dropped_row_counts_attributes_not_class_names;
#[path = "../an_attribute_less_div_unwraps_to_its_content.rs"]
mod an_attribute_less_div_unwraps_to_its_content;
#[path = "../an_authored_comment_cannot_impersonate_a_definition_placeholder.rs"]
mod an_authored_comment_cannot_impersonate_a_definition_placeholder;
#[path = "../an_authored_paragraph_around_a_lone_image_is_a_declared_loss.rs"]
mod an_authored_paragraph_around_a_lone_image_is_a_declared_loss;
#[path = "../an_authored_paragraph_is_trimmed_like_a_synthesized_one.rs"]
mod an_authored_paragraph_is_trimmed_like_a_synthesized_one;
#[path = "../an_emphasis_emptied_by_a_dropped_code_span_is_dropped.rs"]
mod an_emphasis_emptied_by_a_dropped_code_span_is_dropped;
#[path = "../an_emphasis_ending_in_a_hard_break_keeps_its_closer.rs"]
mod an_emphasis_ending_in_a_hard_break_keeps_its_closer;
#[path = "../an_emptied_marker_writes_no_trailing_space.rs"]
mod an_emptied_marker_writes_no_trailing_space;
#[path = "../an_empty_body_claims_no_line_below_column_0.rs"]
mod an_empty_body_claims_no_line_below_column_0;
#[path = "../an_empty_brace_pair_is_text.rs"]
mod an_empty_brace_pair_is_text;
#[path = "../an_empty_caption_is_not_a_caption_line.rs"]
mod an_empty_caption_is_not_a_caption_line;
#[path = "../an_empty_description_body_is_written_with_the_sentinel.rs"]
mod an_empty_description_body_is_written_with_the_sentinel;
#[path = "../an_empty_footnote_body_is_written_with_the_empty_sentinel.rs"]
mod an_empty_footnote_body_is_written_with_the_empty_sentinel;
#[path = "../an_empty_footnote_body_still_carries_the_backlink.rs"]
mod an_empty_footnote_body_still_carries_the_backlink;
#[path = "../an_empty_inline_element_is_dropped_silently.rs"]
mod an_empty_inline_element_is_dropped_silently;
#[path = "../an_empty_list_is_dropped.rs"]
mod an_empty_list_is_dropped;
#[path = "../an_empty_or_refused_class_value_adds_no_token.rs"]
mod an_empty_or_refused_class_value_adds_no_token;
#[path = "../an_empty_or_whitespace_edged_mark.rs"]
mod an_empty_or_whitespace_edged_mark;
#[path = "../an_empty_term_marker_in_a_description_body_is_text.rs"]
mod an_empty_term_marker_in_a_description_body_is_text;
#[path = "../an_empty_unsupported_element_is_dropped_not_unwrapped.rs"]
mod an_empty_unsupported_element_is_dropped_not_unwrapped;
#[path = "../an_endnotes_section_keeps_the_position_it_was_written_at.rs"]
mod an_endnotes_section_keeps_the_position_it_was_written_at;
#[path = "../an_escalation_reaches_the_block_that_failed.rs"]
mod an_escalation_reaches_the_block_that_failed;
#[path = "../an_escaped_attribute_block_escapes_what_opens_it.rs"]
mod an_escaped_attribute_block_escapes_what_opens_it;
#[path = "../an_escaped_closing_pipe_is_an_escape.rs"]
mod an_escaped_closing_pipe_is_an_escape;
#[path = "../an_escaped_marker_ends_no_delimiter_run.rs"]
mod an_escaped_marker_ends_no_delimiter_run;
#[path = "../an_escaped_marker_leaves_the_next_run_spelled.rs"]
mod an_escaped_marker_leaves_the_next_run_spelled;
#[path = "../an_html_comment_imports_as_a_carve_comment.rs"]
mod an_html_comment_imports_as_a_carve_comment;
#[path = "../an_html_import_drops_a_denied_scheme_destination.rs"]
mod an_html_import_drops_a_denied_scheme_destination;
#[path = "../an_html_import_keeps_every_attribute_the_language_can_hold.rs"]
mod an_html_import_keeps_every_attribute_the_language_can_hold;
#[path = "../an_import_moves_edge_space_out_and_reads_linear_mathml.rs"]
mod an_import_moves_edge_space_out_and_reads_linear_mathml;
#[path = "../an_imported_cell_and_term_drop_their_edge_space.rs"]
mod an_imported_cell_and_term_drop_their_edge_space;
#[path = "../an_imported_container_comes_back_as_the_container.rs"]
mod an_imported_container_comes_back_as_the_container;
#[path = "../an_imported_element_keeps_its_key_order.rs"]
mod an_imported_element_keeps_its_key_order;
#[path = "../an_imported_hard_break_drops_the_space_after_it.rs"]
mod an_imported_hard_break_drops_the_space_after_it;
#[path = "../an_included_child_is_read_with_the_callers_extensions.rs"]
mod an_included_child_is_read_with_the_callers_extensions;
#[path = "../an_indented_code_span_starts_at_its_backtick.rs"]
mod an_indented_code_span_starts_at_its_backtick;
#[path = "../an_indented_lone_image_is_a_paragraph.rs"]
mod an_indented_lone_image_is_a_paragraph;
#[path = "../an_ingested_default_start_is_not_re_emitted.rs"]
mod an_ingested_default_start_is_not_re_emitted;
#[path = "../an_ingested_start_of_one_is_the_default.rs"]
mod an_ingested_start_of_one_is_the_default;
#[path = "../an_inline_include_leaves_one_text_run.rs"]
mod an_inline_include_leaves_one_text_run;
#[path = "../an_invalid_attribute_block_is_not_a_definition.rs"]
mod an_invalid_attribute_block_is_not_a_definition;
#[path = "../an_invisible_line_below_a_description_s_column_folds_as_text.rs"]
mod an_invisible_line_below_a_description_s_column_folds_as_text;
#[path = "../an_italic_wrapping_a_strong_keeps_its_nesting.rs"]
mod an_italic_wrapping_a_strong_keeps_its_nesting;
#[path = "../an_item_opening_with_a_note_reference_keeps_its_text.rs"]
mod an_item_opening_with_a_note_reference_keeps_its_text;
#[path = "../an_items_attribute_block_moves_its_content_column.rs"]
mod an_items_attribute_block_moves_its_content_column;
#[path = "../an_items_fence_closer_is_searched_in_its_own_body.rs"]
mod an_items_fence_closer_is_searched_in_its_own_body;
#[path = "../an_opener_at_or_past_a_description_bodys_column_closes_its_paragraph.rs"]
mod an_opener_at_or_past_a_description_bodys_column_closes_its_paragraph;
#[path = "../an_opener_of_an_open_kind_is_literal.rs"]
mod an_opener_of_an_open_kind_is_literal;
#[path = "../an_ordered_html_task_item_declares_its_lost_box.rs"]
mod an_ordered_html_task_item_declares_its_lost_box;
#[path = "../an_ordered_task_item_keeps_its_label.rs"]
mod an_ordered_task_item_keeps_its_label;
#[path = "../an_over_indented_unterminated_fence_folds.rs"]
mod an_over_indented_unterminated_fence_folds;
#[path = "../an_unattached_block_attribute_is_reported.rs"]
mod an_unattached_block_attribute_is_reported;
#[path = "../an_unclosed_run_at_a_forced_closer_drops_the_break.rs"]
mod an_unclosed_run_at_a_forced_closer_drops_the_break;
#[path = "../an_unclosed_verbatim_run_strips_at_a_forced_closer.rs"]
mod an_unclosed_verbatim_run_strips_at_a_forced_closer;
#[path = "../an_underscore_pair_in_text_is_escaped_where_the_block_would_pair_it.rs"]
mod an_underscore_pair_in_text_is_escaped_where_the_block_would_pair_it;
#[path = "../an_unfinished_fence_on_a_nested_description_lead_owns_its_body.rs"]
mod an_unfinished_fence_on_a_nested_description_lead_owns_its_body;
#[path = "../an_unmarked_list_marker_under_a_quoted_item_folds_as_lazy_text.rs"]
mod an_unmarked_list_marker_under_a_quoted_item_folds_as_lazy_text;
#[path = "../an_unresolved_include_names_where_the_file_would_be.rs"]
mod an_unresolved_include_names_where_the_file_would_be;
#[path = "../an_unsatisfied_continuation_marker_is_text.rs"]
mod an_unsatisfied_continuation_marker_is_text;
#[path = "../an_unspellable_attribute_name_is_reported_in_the_ruled_words.rs"]
mod an_unspellable_attribute_name_is_reported_in_the_ruled_words;
#[path = "../an_unspellable_block_does_not_cancel_list_adjacency.rs"]
mod an_unspellable_block_does_not_cancel_list_adjacency;
#[path = "../an_unspellable_container_title_moves_into_the_body.rs"]
mod an_unspellable_container_title_moves_into_the_body;
#[path = "../an_unspellable_empty_code_span_is_refused.rs"]
mod an_unspellable_empty_code_span_is_refused;
#[path = "../an_unsupported_element_is_replaced_by_its_children.rs"]
mod an_unsupported_element_is_replaced_by_its_children;
#[path = "../an_unterminated_figure_group_closes_at_end_of_input.rs"]
mod an_unterminated_figure_group_closes_at_end_of_input;
#[path = "../an_untyped_wire_record_is_closed_too.rs"]
mod an_untyped_wire_record_is_closed_too;
#[path = "../ansi_cjk_width.rs"]
mod ansi_cjk_width;
#[path = "../ansi_destination_denylist.rs"]
mod ansi_destination_denylist;
#[path = "../ansi_ragged_table.rs"]
mod ansi_ragged_table;
#[path = "../ansi_smart_typography_mode.rs"]
mod ansi_smart_typography_mode;
#[path = "../ascii_heading_ids.rs"]
mod ascii_heading_ids;
#[path = "../ast_envelope.rs"]
mod ast_envelope;
#[path = "../ast_json.rs"]
mod ast_json;
#[path = "../ast_json_error_details.rs"]
mod ast_json_error_details;
#[path = "../ast_json_roundtrip_depth.rs"]
mod ast_json_roundtrip_depth;
#[path = "../ast_merge.rs"]
mod ast_merge;
#[path = "../ast_patch.rs"]
mod ast_patch;
#[path = "../ast_whitespace_and_annotations.rs"]
mod ast_whitespace_and_annotations;
#[path = "../attr_identifier.rs"]
mod attr_identifier;
#[path = "../attr_line_after_a_continuation_marker.rs"]
mod attr_line_after_a_continuation_marker;
#[path = "../attr_line_before_a_sublist.rs"]
mod attr_line_before_a_sublist;
#[path = "../attributes_on_a_mention_or_tag_are_unspellable.rs"]
mod attributes_on_a_mention_or_tag_are_unspellable;
#[path = "../authored_abbr_on_every_target.rs"]
mod authored_abbr_on_every_target;
#[path = "../authored_base_non_definition_edges.rs"]
mod authored_base_non_definition_edges;
#[path = "../authored_body_block_base.rs"]
mod authored_body_block_base;
#[path = "../authored_sentinels_survive_fmt.rs"]
mod authored_sentinels_survive_fmt;
#[path = "../autolink_url_char_classes.rs"]
mod autolink_url_char_classes;
#[path = "../autolinks.rs"]
mod autolinks;
#[path = "../bare_dot_ordered_marker.rs"]
mod bare_dot_ordered_marker;
#[path = "../bbcode_formatting_spells_like_the_writer.rs"]
mod bbcode_formatting_spells_like_the_writer;
#[path = "../bbcode_migrate.rs"]
mod bbcode_migrate;
#[path = "../bbcode_text_beside_tags.rs"]
mod bbcode_text_beside_tags;
#[path = "../below_a_definition_body_s_column_the_kinds_answer_one_rule.rs"]
mod below_a_definition_body_s_column_the_kinds_answer_one_rule;
#[path = "../below_the_definition_bodys_column_the_body_ends.rs"]
mod below_the_definition_bodys_column_the_body_ends;
#[path = "../block_attr_lines.rs"]
mod block_attr_lines;
#[path = "../block_image_is_a_resolved_tree_property.rs"]
mod block_image_is_a_resolved_tree_property;
#[path = "../block_positions.rs"]
mod block_positions;
#[path = "../blockquote_continuation_marker.rs"]
mod blockquote_continuation_marker;
#[path = "../bold_italic_authored_form.rs"]
mod bold_italic_authored_form;
#[path = "../boolean_attrs.rs"]
mod boolean_attrs;
#[path = "../both_parse_paths_agree_on_fence_content.rs"]
mod both_parse_paths_agree_on_fence_content;
#[path = "../builtin_extensions.rs"]
mod builtin_extensions;
#[path = "../c0_controls_on_the_render_targets.rs"]
mod c0_controls_on_the_render_targets;
#[path = "../canonical_fence_info_carries_no_space.rs"]
mod canonical_fence_info_carries_no_space;
#[path = "../canonical_vocabulary_matches_the_spec.rs"]
mod canonical_vocabulary_matches_the_spec;
#[path = "../canonical_writer_emits_no_indented_blank_line.rs"]
mod canonical_writer_emits_no_indented_blank_line;
#[path = "../caption_inline_positions.rs"]
mod caption_inline_positions;
#[path = "../caption_inside_a_list_item.rs"]
mod caption_inside_a_list_item;
#[path = "../caption_span_columns_and_marker_width.rs"]
mod caption_span_columns_and_marker_width;
#[path = "../caption_span_excludes_the_marker.rs"]
mod caption_span_excludes_the_marker;
#[path = "../caret_label_is_a_link_reference.rs"]
mod caret_label_is_a_link_reference;
#[path = "../cell_attributes_bind_last.rs"]
mod cell_attributes_bind_last;
#[path = "../cell_text_align_in_every_mode.rs"]
mod cell_text_align_in_every_mode;
#[path = "../checked_ast_operations.rs"]
mod checked_ast_operations;
#[path = "../citations.rs"]
mod citations;
#[path = "../cli_includes.rs"]
mod cli_includes;
#[path = "../cli_lint.rs"]
mod cli_lint;
#[path = "../cli_merge.rs"]
mod cli_merge;
#[path = "../cli_migrate_from.rs"]
mod cli_migrate_from;
#[path = "../cli_no_raw_html.rs"]
mod cli_no_raw_html;
#[path = "../cli_non_utf8_argument.rs"]
mod cli_non_utf8_argument;
#[path = "../cli_quote_locale.rs"]
mod cli_quote_locale;
#[path = "../cli_render_loss.rs"]
mod cli_render_loss;
#[path = "../cli_render_options.rs"]
mod cli_render_options;
#[path = "../cli_smart_typography.rs"]
mod cli_smart_typography;
#[path = "../cli_stamp_query.rs"]
mod cli_stamp_query;
#[path = "../code_callouts.rs"]
mod code_callouts;
#[path = "../code_fences_close_only_at_container_or_authored_base.rs"]
mod code_fences_close_only_at_container_or_authored_base;
#[path = "../code_title_edges.rs"]
mod code_title_edges;
#[path = "../collapsed_reference_publishes_its_resolution_key.rs"]
mod collapsed_reference_publishes_its_resolution_key;
#[path = "../collected_definition_leaves_no_comment_node.rs"]
mod collected_definition_leaves_no_comment_node;
#[path = "../collected_definitions_are_in_source_order.rs"]
mod collected_definitions_are_in_source_order;
#[path = "../colon_fence_attrs.rs"]
mod colon_fence_attrs;
#[path = "../colon_fence_authored_columns.rs"]
mod colon_fence_authored_columns;
#[path = "../colon_fence_metadata_slots_are_a_space.rs"]
mod colon_fence_metadata_slots_are_a_space;
#[path = "../colon_fence_separator_is_a_space.rs"]
mod colon_fence_separator_is_a_space;
#[path = "../comment_at_column_zero_keeps_item_open.rs"]
mod comment_at_column_zero_keeps_item_open;
#[path = "../comment_below_content_column_keeps_item_open.rs"]
mod comment_below_content_column_keeps_item_open;
#[path = "../comment_body_is_relative_to_its_fence.rs"]
mod comment_body_is_relative_to_its_fence;
#[path = "../comment_does_not_revive_a_closed_item.rs"]
mod comment_does_not_revive_a_closed_item;
#[path = "../comment_in_list_item.rs"]
mod comment_in_list_item;
#[path = "../comment_span_opener_column.rs"]
mod comment_span_opener_column;
#[path = "../comparable_footnote_defs.rs"]
mod comparable_footnote_defs;
#[path = "../conformance_regressions.rs"]
mod conformance_regressions;
#[path = "../constructed_figure_invariants.rs"]
mod constructed_figure_invariants;
#[path = "../container_scoped_content_columns.rs"]
mod container_scoped_content_columns;
#[path = "../corpus.rs"]
mod corpus;
#[path = "../corpus_canonical_form.rs"]
mod corpus_canonical_form;
#[path = "../corpus_render_fixtures.rs"]
mod corpus_render_fixtures;
#[path = "../corpus_render_ledger.rs"]
mod corpus_render_ledger;
#[path = "../crossref_href.rs"]
mod crossref_href;
#[path = "../crossref_label_budget.rs"]
mod crossref_label_budget;
#[path = "../crossref_label_is_the_headings_nodes.rs"]
mod crossref_label_is_the_headings_nodes;
#[path = "../crossrefs.rs"]
mod crossrefs;
#[path = "../dash_run_decomposition.rs"]
mod dash_run_decomposition;
#[path = "../deep_document_drop.rs"]
mod deep_document_drop;
#[path = "../deep_markdown_is_refused_not_aborted.rs"]
mod deep_markdown_is_refused_not_aborted;
#[path = "../definition_at_document_column_closes_list.rs"]
mod definition_at_document_column_closes_list;
#[path = "../definition_at_inner_content_column.rs"]
mod definition_at_inner_content_column;
#[path = "../definition_attrs_reach_a_reference_image.rs"]
mod definition_attrs_reach_a_reference_image;
#[path = "../definition_body_canonical_space.rs"]
mod definition_body_canonical_space;
#[path = "../definition_continuation_column.rs"]
mod definition_continuation_column;
#[path = "../definition_continuation_columns.rs"]
mod definition_continuation_columns;
#[path = "../definition_in_a_quote_inside_an_item.rs"]
mod definition_in_a_quote_inside_an_item;
#[path = "../definition_in_a_quoted_item.rs"]
mod definition_in_a_quoted_item;
#[path = "../definition_inside_a_dd_test.rs"]
mod definition_inside_a_dd_test;
#[path = "../definition_inside_comment.rs"]
mod definition_inside_comment;
#[path = "../definition_marker_separator_run.rs"]
mod definition_marker_separator_run;
#[path = "../definition_prepass_fence_reach.rs"]
mod definition_prepass_fence_reach;
#[path = "../definition_sentinel_boundary.rs"]
mod definition_sentinel_boundary;
#[path = "../definition_term_positions.rs"]
mod definition_term_positions;
#[path = "../definition_term_separator_space.rs"]
mod definition_term_separator_space;
#[path = "../definition_written_back_on_its_description_line.rs"]
mod definition_written_back_on_its_description_line;
#[path = "../definition_written_back_where_it_was.rs"]
mod definition_written_back_where_it_was;
#[path = "../delimited_inline_comments.rs"]
mod delimited_inline_comments;
#[path = "../denied_definition_injects_no_marker.rs"]
mod denied_definition_injects_no_marker;
#[path = "../denied_footnote_loses_its_number.rs"]
mod denied_footnote_loses_its_number;
#[path = "../deny_root_field_types.rs"]
mod deny_root_field_types;
#[path = "../derived_display_text_clones_the_nodes.rs"]
mod derived_display_text_clones_the_nodes;
#[path = "../description_column_closes_overindented_code_fences.rs"]
mod description_column_closes_overindented_code_fences;
#[path = "../description_fence_paragraph_boundary.rs"]
mod description_fence_paragraph_boundary;
#[path = "../destination_parens.rs"]
mod destination_parens;
#[path = "../destination_unicode_whitespace.rs"]
mod destination_unicode_whitespace;
#[path = "../details.rs"]
mod details;
#[path = "../differential_audit_regressions.rs"]
mod differential_audit_regressions;
#[path = "../digit_leading_explicit_identifiers.rs"]
mod digit_leading_explicit_identifiers;
#[path = "../directive_titles.rs"]
mod directive_titles;
#[path = "../document_summary.rs"]
mod document_summary;
#[path = "../empty_block_contributes_no_line.rs"]
mod empty_block_contributes_no_line;
#[path = "../empty_container_body.rs"]
mod empty_container_body;
#[path = "../empty_footnote_definition_position.rs"]
mod empty_footnote_definition_position;
#[path = "../empty_item_paragraph_test.rs"]
mod empty_item_paragraph_test;
#[path = "../empty_raw_inline_refuses.rs"]
mod empty_raw_inline_refuses;
#[path = "../empty_reference_label.rs"]
mod empty_reference_label;
#[path = "../engine_pin_guard.rs"]
mod engine_pin_guard;
#[path = "../escape_comparison_sees_editorial.rs"]
mod escape_comparison_sees_editorial;
#[path = "../escaped_caret_publishes_the_character.rs"]
mod escaped_caret_publishes_the_character;
#[path = "../escaped_text_node.rs"]
mod escaped_text_node;
#[path = "../every_derived_display_text_clones_the_nodes.rs"]
mod every_derived_display_text_clones_the_nodes;
#[path = "../every_element_the_raw_keep_path_reaches_reports_the_same_way.rs"]
mod every_element_the_raw_keep_path_reaches_reports_the_same_way;
#[path = "../every_table_cell_pads_its_content.rs"]
mod every_table_cell_pads_its_content;
#[path = "../explicit_heading_id_collision.rs"]
mod explicit_heading_id_collision;
#[path = "../extension_id_namespace.rs"]
mod extension_id_namespace;
#[path = "../extensions.rs"]
mod extensions;
#[path = "../fence_interior_blank_looseness.rs"]
mod fence_interior_blank_looseness;
#[path = "../fenced_body_below_the_content_column.rs"]
mod fenced_body_below_the_content_column;
#[path = "../fenced_code_columns.rs"]
mod fenced_code_columns;
#[path = "../fenced_code_info.rs"]
mod fenced_code_info;
#[path = "../fenced_definition_body_below_the_column.rs"]
mod fenced_definition_body_below_the_column;
#[path = "../floating_attrs_below_list_content_column.rs"]
mod floating_attrs_below_list_content_column;
#[path = "../flush_left_line_after_a_div_in_a_container.rs"]
mod flush_left_line_after_a_div_in_a_container;
#[path = "../fmt_comment_blocks.rs"]
mod fmt_comment_blocks;
#[path = "../fmt_frontmatter_is_never_manufactured.rs"]
mod fmt_frontmatter_is_never_manufactured;
#[path = "../fmt_heading_single_line.rs"]
mod fmt_heading_single_line;
#[path = "../fmt_list_indent.rs"]
mod fmt_list_indent;
#[path = "../fmt_span_marker_padding.rs"]
mod fmt_span_marker_padding;
#[path = "../fmt_writes_a_trailing_comment_once.rs"]
mod fmt_writes_a_trailing_comment_once;
#[path = "../folded_fenced_comments_keep_their_block_flag.rs"]
mod folded_fenced_comments_keep_their_block_flag;
#[path = "../folded_term_comment_break_positions.rs"]
mod folded_term_comment_break_positions;
#[path = "../footnote_backlink_paragraph_test.rs"]
mod footnote_backlink_paragraph_test;
#[path = "../footnote_body_indent_is_relative.rs"]
mod footnote_body_indent_is_relative;
#[path = "../footnote_continuation_column.rs"]
mod footnote_continuation_column;
#[path = "../footnote_def_line_validation.rs"]
mod footnote_def_line_validation;
#[path = "../footnote_def_span.rs"]
mod footnote_def_span;
#[path = "../footnote_label_is_single_line.rs"]
mod footnote_label_is_single_line;
#[path = "../footnote_pairing_boundaries.rs"]
mod footnote_pairing_boundaries;
#[path = "../footnote_ref_id_never_crosses_the_wire.rs"]
mod footnote_ref_id_never_crosses_the_wire;
#[path = "../footnote_ref_number_is_serialized.rs"]
mod footnote_ref_number_is_serialized;
#[path = "../formatter_parity_regressions.rs"]
mod formatter_parity_regressions;
#[path = "../formatting_around_a_link_keeps_only_separating_space.rs"]
mod formatting_around_a_link_keeps_only_separating_space;
#[path = "../frontmatter_opener_carries_its_format_token.rs"]
mod frontmatter_opener_carries_its_format_token;
#[path = "../frontmatter_opener_slot_is_a_space.rs"]
mod frontmatter_opener_slot_is_a_space;
#[path = "../frontmatter_raw.rs"]
mod frontmatter_raw;
#[path = "../generated_heading_id_is_published.rs"]
mod generated_heading_id_is_published;
#[path = "../gfm_alignment_on_header_only.rs"]
mod gfm_alignment_on_header_only;
#[path = "../glossary.rs"]
mod glossary;
#[path = "../glued_colon_fence_paragraph.rs"]
mod glued_colon_fence_paragraph;
#[path = "../hard_break_span.rs"]
mod hard_break_span;
#[path = "../hardbreaks_fence_positions.rs"]
mod hardbreaks_fence_positions;
#[path = "../heading_caption_whitespace.rs"]
mod heading_caption_whitespace;
#[path = "../heading_folds_lazy.rs"]
mod heading_folds_lazy;
#[path = "../heading_interrupt.rs"]
mod heading_interrupt;
#[path = "../heading_nbsp_on_markdown.rs"]
mod heading_nbsp_on_markdown;
#[path = "../heading_numbers.rs"]
mod heading_numbers;
#[path = "../heading_ref_nfc.rs"]
mod heading_ref_nfc;
#[path = "../heading_reference_authored_form.rs"]
mod heading_reference_authored_form;
#[path = "../html_code_languages.rs"]
mod html_code_languages;
#[path = "../html_import.rs"]
mod html_import;
#[path = "../html_import_authored_heading_id.rs"]
mod html_import_authored_heading_id;
#[path = "../html_import_checkbox_type_case.rs"]
mod html_import_checkbox_type_case;
#[path = "../html_import_definition_list.rs"]
mod html_import_definition_list;
#[path = "../html_import_details_and_quote.rs"]
mod html_import_details_and_quote;
#[path = "../html_import_hard_depth.rs"]
mod html_import_hard_depth;
#[path = "../html_import_ins_and_ol_type.rs"]
mod html_import_ins_and_ol_type;
#[path = "../html_import_list_tightness.rs"]
mod html_import_list_tightness;
#[path = "../html_import_mathml.rs"]
mod html_import_mathml;
#[path = "../html_import_roundtrip_heading_id.rs"]
mod html_import_roundtrip_heading_id;
#[path = "../html_import_roundtrip_sectioning_wrappers.rs"]
mod html_import_roundtrip_sectioning_wrappers;
#[path = "../html_import_table_row_groups.rs"]
mod html_import_table_row_groups;
#[path = "../html_import_table_spans.rs"]
mod html_import_table_spans;
#[path = "../html_import_task_item_checkbox.rs"]
mod html_import_task_item_checkbox;
#[path = "../html_smart_typography_mode.rs"]
mod html_smart_typography_mode;
#[path = "../img_fence.rs"]
mod img_fence;
#[path = "../implicit_heading_ref_explicit_id.rs"]
mod implicit_heading_ref_explicit_id;
#[path = "../implicit_heading_reference.rs"]
mod implicit_heading_reference;
#[path = "../import_attribute_policy_parity.rs"]
mod import_attribute_policy_parity;
#[path = "../import_fixed_point_contract_shapes.rs"]
mod import_fixed_point_contract_shapes;
#[path = "../include_security_conformance.rs"]
mod include_security_conformance;
#[path = "../incremental_parse.rs"]
mod incremental_parse;
#[path = "../indented_comment_fence_hides_its_body.rs"]
mod indented_comment_fence_hides_its_body;
#[path = "../indented_fence_marker_payload.rs"]
mod indented_fence_marker_payload;
#[path = "../index_terms.rs"]
mod index_terms;
#[path = "../ingest_bounds_are_measured.rs"]
mod ingest_bounds_are_measured;
#[path = "../ingest_validates_the_whole_payload.rs"]
mod ingest_validates_the_whole_payload;
#[path = "../ingested_footnote_number_is_rederived.rs"]
mod ingested_footnote_number_is_rederived;
#[path = "../inline_attribute_interior_is_space_only.rs"]
mod inline_attribute_interior_is_space_only;
#[path = "../inline_code_attr.rs"]
mod inline_code_attr;
#[path = "../inline_comment_node.rs"]
mod inline_comment_node;
#[path = "../inline_literal.rs"]
mod inline_literal;
#[path = "../invisible_line_does_not_cancel_separation.rs"]
mod invisible_line_does_not_cancel_separation;
#[path = "../item_comment_below_a_fenced_body.rs"]
mod item_comment_below_a_fenced_body;
#[path = "../item_link_reference_definition.rs"]
mod item_link_reference_definition;
#[path = "../label_scan_editorial_comment.rs"]
mod label_scan_editorial_comment;
#[path = "../label_whitespace_key.rs"]
mod label_whitespace_key;
#[path = "../language_attribute.rs"]
mod language_attribute;
#[path = "../lazy_fold_before_an_abbreviation.rs"]
mod lazy_fold_before_an_abbreviation;
#[path = "../lazy_framing_never_leaks.rs"]
mod lazy_framing_never_leaks;
#[path = "../leftover_plus_line_test.rs"]
mod leftover_plus_line_test;
#[path = "../line_block_and_arrow.rs"]
mod line_block_and_arrow;
#[path = "../line_block_break_positions.rs"]
mod line_block_break_positions;
#[path = "../line_block_keeps_definition_shaped_lines.rs"]
mod line_block_keeps_definition_shaped_lines;
#[path = "../line_block_lines.rs"]
mod line_block_lines;
#[path = "../line_block_medial_gaps.rs"]
mod line_block_medial_gaps;
#[path = "../line_block_node.rs"]
mod line_block_node;
#[path = "../line_block_tab_spans.rs"]
mod line_block_tab_spans;
#[path = "../line_block_thematic_padding.rs"]
mod line_block_thematic_padding;
#[path = "../link_def_prepass_line_block_column.rs"]
mod link_def_prepass_line_block_column;
#[path = "../link_definition_pos_from_a_footnote_body.rs"]
mod link_definition_pos_from_a_footnote_body;
#[path = "../link_title_attribute_order.rs"]
mod link_title_attribute_order;
#[path = "../link_title_escaped_quote.rs"]
mod link_title_escaped_quote;
#[path = "../link_title_slot_is_a_space.rs"]
mod link_title_slot_is_a_space;
#[path = "../lint_reference_rules.rs"]
mod lint_reference_rules;
#[path = "../lint_semantic_attributes.rs"]
mod lint_semantic_attributes;
#[path = "../lint_source_rules.rs"]
mod lint_source_rules;
#[path = "../list_indent_model_a.rs"]
mod list_indent_model_a;
#[path = "../list_item_attribute_line.rs"]
mod list_item_attribute_line;
#[path = "../list_item_attributes.rs"]
mod list_item_attributes;
#[path = "../list_item_authored_block_base.rs"]
mod list_item_authored_block_base;
#[path = "../list_item_span_contains_children.rs"]
mod list_item_span_contains_children;
#[path = "../list_lazy_after_sublist.rs"]
mod list_lazy_after_sublist;
#[path = "../list_line_block_indentation.rs"]
mod list_line_block_indentation;
#[path = "../list_table.rs"]
mod list_table;
#[path = "../markdown_authored_escape_narrows_on_the_line.rs"]
mod markdown_authored_escape_narrows_on_the_line;
#[path = "../markdown_bullet.rs"]
mod markdown_bullet;
#[path = "../markdown_crossref_heading_ids.rs"]
mod markdown_crossref_heading_ids;
#[path = "../markdown_delimiter_matches_the_header_row.rs"]
mod markdown_delimiter_matches_the_header_row;
#[path = "../markdown_destination_probe.rs"]
mod markdown_destination_probe;
#[path = "../markdown_emphasis_pads_outside_the_delimiters.rs"]
mod markdown_emphasis_pads_outside_the_delimiters;
#[path = "../markdown_escapes_embedded_html.rs"]
mod markdown_escapes_embedded_html;
#[path = "../markdown_fence_effective_title.rs"]
mod markdown_fence_effective_title;
#[path = "../markdown_hard_break_backslash.rs"]
mod markdown_hard_break_backslash;
#[path = "../markdown_html_block_in_container.rs"]
mod markdown_html_block_in_container;
#[path = "../markdown_import_regressions.rs"]
mod markdown_import_regressions;
#[path = "../markdown_inline_html_edges.rs"]
mod markdown_inline_html_edges;
#[path = "../markdown_list_tightness_reads_back.rs"]
mod markdown_list_tightness_reads_back;
#[path = "../markdown_nested_list_lazy_seam.rs"]
mod markdown_nested_list_lazy_seam;
#[path = "../markdown_ordered_delimiter.rs"]
mod markdown_ordered_delimiter;
#[path = "../markdown_same_kind_nesting_unwraps.rs"]
mod markdown_same_kind_nesting_unwraps;
#[path = "../markdown_smart_typography_mode.rs"]
mod markdown_smart_typography_mode;
#[path = "../markdown_table_alignment.rs"]
mod markdown_table_alignment;
#[path = "../markdown_task_item_continuation_pad.rs"]
mod markdown_task_item_continuation_pad;
#[path = "../markdown_tight_item_inline_run.rs"]
mod markdown_tight_item_inline_run;
#[path = "../markdown_title_needs_language.rs"]
mod markdown_title_needs_language;
#[path = "../markdown_unclosed_inline_html.rs"]
mod markdown_unclosed_inline_html;
#[path = "../markdown_underscore_escapes.rs"]
mod markdown_underscore_escapes;
#[path = "../markdown_url_denylist_full.rs"]
mod markdown_url_denylist_full;
#[path = "../markdown_writer_targets_fixture.rs"]
mod markdown_writer_targets_fixture;
#[path = "../marker_attributes_do_not_move_content_column.rs"]
mod marker_attributes_do_not_move_content_column;
#[path = "../marker_inside_an_open_fence_is_code_text.rs"]
mod marker_inside_an_open_fence_is_code_text;
#[path = "../marker_line_blank_continuation.rs"]
mod marker_line_blank_continuation;
#[path = "../marker_line_definition_term.rs"]
mod marker_line_definition_term;
#[path = "../marker_line_heading.rs"]
mod marker_line_heading;
#[path = "../marker_line_lead_looseness.rs"]
mod marker_line_lead_looseness;
#[path = "../marker_line_quote_lazy.rs"]
mod marker_line_quote_lazy;
#[path = "../marker_line_sublist.rs"]
mod marker_line_sublist;
#[path = "../marker_line_unterminated_fence.rs"]
mod marker_line_unterminated_fence;
#[path = "../md_table_alignment.rs"]
mod md_table_alignment;
#[path = "../mention_attributes.rs"]
mod mention_attributes;
#[path = "../mention_boundaries.rs"]
mod mention_boundaries;
#[path = "../migration_result.rs"]
mod migration_result;
#[path = "../multiline_captions.rs"]
mod multiline_captions;
#[path = "../nbsp_escape_positions.rs"]
mod nbsp_escape_positions;
#[path = "../nbsp_in_presentation_writers.rs"]
mod nbsp_in_presentation_writers;
#[path = "../nbsp_placeholder_codepoint.rs"]
mod nbsp_placeholder_codepoint;
#[path = "../nested_lazy_continuation.rs"]
mod nested_lazy_continuation;
#[path = "../nested_marker_line_lazy_resume.rs"]
mod nested_marker_line_lazy_resume;
#[path = "../nesting_cap_flattening.rs"]
mod nesting_cap_flattening;
#[path = "../no_nested_links.rs"]
mod no_nested_links;
#[path = "../no_single_variant_sets_the_size_of_every_block_node.rs"]
mod no_single_variant_sets_the_size_of_every_block_node;
#[path = "../no_trailing_whitespace.rs"]
mod no_trailing_whitespace;
#[path = "../non_html_parity.rs"]
mod non_html_parity;
#[path = "../non_html_renderers_keep_source_order.rs"]
mod non_html_renderers_keep_source_order;
#[path = "../non_html_security.rs"]
mod non_html_security;
#[path = "../note_body_definition.rs"]
mod note_body_definition;
#[path = "../note_body_keeps_relative_indent.rs"]
mod note_body_keeps_relative_indent;
#[path = "../nothing_sits_before_the_link_tails_closing_parenthesis.rs"]
mod nothing_sits_before_the_link_tails_closing_parenthesis;
#[path = "../numbered_listing_equation.rs"]
mod numbered_listing_equation;
#[path = "../one_consumed_boolean_spells_the_looseness.rs"]
mod one_consumed_boolean_spells_the_looseness;
#[path = "../one_whitespace_definition.rs"]
mod one_whitespace_definition;
#[path = "../opaque_span_in_container.rs"]
mod opaque_span_in_container;
#[path = "../open_issue_regressions.rs"]
mod open_issue_regressions;
#[path = "../optional_corpus.rs"]
mod optional_corpus;
#[path = "../ordered_list_dialect.rs"]
mod ordered_list_dialect;
#[path = "../over_cap_openers_stay_literal.rs"]
mod over_cap_openers_stay_literal;
#[path = "../over_cap_paragraph_position.rs"]
mod over_cap_paragraph_position;
#[path = "../padding_slots_take_one_space.rs"]
mod padding_slots_take_one_space;
#[path = "../paragraph_interruption.rs"]
mod paragraph_interruption;
#[path = "../paragraph_trailing_whitespace.rs"]
mod paragraph_trailing_whitespace;
#[path = "../plain_keeps_leading_cell.rs"]
mod plain_keeps_leading_cell;
#[path = "../plain_smart_typography_mode.rs"]
mod plain_smart_typography_mode;
#[path = "../plain_unresolved_footnote.rs"]
mod plain_unresolved_footnote;
#[path = "../position_spans_match_source.rs"]
mod position_spans_match_source;
#[path = "../positions_index_the_original_file.rs"]
mod positions_index_the_original_file;
#[path = "../positions_on_the_published_only_nodes.rs"]
mod positions_on_the_published_only_nodes;
#[path = "../post_blank_content_column.rs"]
mod post_blank_content_column;
#[path = "../post_blank_line_below_the_content_column.rs"]
mod post_blank_line_below_the_content_column;
#[path = "../pre_render_flatteners_keep_the_source_run.rs"]
mod pre_render_flatteners_keep_the_source_run;
#[path = "../prepass_columns_end_at_a_table.rs"]
mod prepass_columns_end_at_a_table;
#[path = "../prepass_sees_an_indented_comment_closer.rs"]
mod prepass_sees_an_indented_comment_closer;
#[path = "../presentation_targets_keep_authored_text.rs"]
mod presentation_targets_keep_authored_text;
#[path = "../profile_fixture_parity.rs"]
mod profile_fixture_parity;
#[path = "../profile_subtypes.rs"]
mod profile_subtypes;
#[path = "../profiles.rs"]
mod profiles;
#[path = "../prosemirror_bridge.rs"]
mod prosemirror_bridge;
#[path = "../published_ast_node_kinds.rs"]
mod published_ast_node_kinds;
#[path = "../quoted_note_body.rs"]
mod quoted_note_body;
#[path = "../quoted_value_stops_at_the_newline.rs"]
mod quoted_value_stops_at_the_newline;
#[path = "../ragged_table_fmt.rs"]
mod ragged_table_fmt;
#[path = "../raised_colon_markers.rs"]
mod raised_colon_markers;
#[path = "../raw_block.rs"]
mod raw_block;
#[path = "../recursion_and_panics.rs"]
mod recursion_and_panics;
#[path = "../reference_definition_anchor.rs"]
mod reference_definition_anchor;
#[path = "../reference_definition_attributes.rs"]
mod reference_definition_attributes;
#[path = "../reference_figure_position.rs"]
mod reference_figure_position;
#[path = "../reference_image_fmt.rs"]
mod reference_image_fmt;
#[path = "../reference_images.rs"]
mod reference_images;
#[path = "../references_placement_followups.rs"]
mod references_placement_followups;
#[path = "../render_carve.rs"]
mod render_carve;
#[path = "../render_ceiling_refuses.rs"]
mod render_ceiling_refuses;
#[path = "../render_loss.rs"]
mod render_loss;
#[path = "../renderer_autolink_display.rs"]
mod renderer_autolink_display;
#[path = "../residual_columns_have_no_source_position.rs"]
mod residual_columns_have_no_source_position;
#[path = "../resolved_reference_keeps_ref.rs"]
mod resolved_reference_keeps_ref;
#[path = "../resolved_table_spans.rs"]
mod resolved_table_spans;
#[path = "../reversible_ast_patch.rs"]
mod reversible_ast_patch;
#[path = "../roll_up_511.rs"]
mod roll_up_511;
#[path = "../root_shape_is_refused_on_ingest.rs"]
mod root_shape_is_refused_on_ingest;
#[path = "../ruby_interchange.rs"]
mod ruby_interchange;
#[path = "../s4_conformance.rs"]
mod s4_conformance;
#[path = "../section_wrapping.rs"]
mod section_wrapping;
#[path = "../security_attributes.rs"]
mod security_attributes;
#[path = "../semantic_span_sugar.rs"]
mod semantic_span_sugar;
#[path = "../short_caption_ast.rs"]
mod short_caption_ast;
#[path = "../smart_punctuation_nodes.rs"]
mod smart_punctuation_nodes;
#[path = "../smart_quotes.rs"]
mod smart_quotes;
#[path = "../social_link_resolvers.rs"]
mod social_link_resolvers;
#[path = "../source_layout.rs"]
mod source_layout;
#[path = "../source_lines.rs"]
mod source_lines;
#[path = "../source_positions.rs"]
mod source_positions;
#[path = "../span_containment_and_opening_markup.rs"]
mod span_containment_and_opening_markup;
#[path = "../spoiler_attribute_order.rs"]
mod spoiler_attribute_order;
#[path = "../stack_floor_attribution.rs"]
mod stack_floor_attribution;
#[path = "../stamp.rs"]
mod stamp;
#[path = "../static_render_mode.rs"]
mod static_render_mode;
#[path = "../streaming_render.rs"]
mod streaming_render;
#[path = "../strict_column0_attr_caption.rs"]
mod strict_column0_attr_caption;
#[path = "../sublist_after_continuation_paragraph.rs"]
mod sublist_after_continuation_paragraph;
#[path = "../sublist_blank_sibling_looseness.rs"]
mod sublist_blank_sibling_looseness;
#[path = "../svg_sanitize.rs"]
mod svg_sanitize;
#[path = "../svg_sanitize_payloads.rs"]
mod svg_sanitize_payloads;
#[path = "../symbols.rs"]
mod symbols;
#[path = "../symmetric_list_interruption.rs"]
mod symmetric_list_interruption;
#[path = "../synthesized_title_order_slot.rs"]
mod synthesized_title_order_slot;
#[path = "../tab_indented_footnote_continuation_positions.rs"]
mod tab_indented_footnote_continuation_positions;
#[path = "../tab_indented_sublist_marker_positions.rs"]
mod tab_indented_sublist_marker_positions;
#[path = "../tab_straddling_the_definition_column.rs"]
mod tab_straddling_the_definition_column;
#[path = "../table_cell_glued_alignment.rs"]
mod table_cell_glued_alignment;
#[path = "../table_cell_inline_positions.rs"]
mod table_cell_inline_positions;
#[path = "../table_cell_padding_is_a_space.rs"]
mod table_cell_padding_is_a_space;
#[path = "../table_column_lint.rs"]
mod table_column_lint;
#[path = "../table_gfm_separator.rs"]
mod table_gfm_separator;
#[path = "../table_head_row_continuations.rs"]
mod table_head_row_continuations;
#[path = "../table_header_rowspan.rs"]
mod table_header_rowspan;
#[path = "../table_orphan_span.rs"]
mod table_orphan_span;
#[path = "../table_row_attributes.rs"]
mod table_row_attributes;
#[path = "../table_row_closing_pipe.rs"]
mod table_row_closing_pipe;
#[path = "../table_row_counts.rs"]
mod table_row_counts;
#[path = "../table_section_attributes.rs"]
mod table_section_attributes;
#[path = "../table_span_covers_caption.rs"]
mod table_span_covers_caption;
#[path = "../task_state_enumeration.rs"]
mod task_state_enumeration;
#[path = "../text_target_structure.rs"]
mod text_target_structure;
#[path = "../text_targets_keep_critic_comment.rs"]
mod text_targets_keep_critic_comment;
#[path = "../text_that_gfm_would_autolink_is_escaped.rs"]
mod text_that_gfm_would_autolink_is_escaped;
#[path = "../the_ansi_quote_bar_reports_containment.rs"]
mod the_ansi_quote_bar_reports_containment;
#[path = "../the_ast_json_ingest_replaces_a_nul.rs"]
mod the_ast_json_ingest_replaces_a_nul;
#[path = "../the_authored_base_belongs_to_the_innermost_open_container.rs"]
mod the_authored_base_belongs_to_the_innermost_open_container;
#[path = "../the_bracket_scan_reaches_what_carve_js_reaches.rs"]
mod the_bracket_scan_reaches_what_carve_js_reaches;
#[path = "../the_canonical_writer_emits_bytes_that_read_back.rs"]
mod the_canonical_writer_emits_bytes_that_read_back;
#[path = "../the_carve_target_answers_the_profile.rs"]
mod the_carve_target_answers_the_profile;
#[path = "../the_continuation_marker_attaches_one_block_in_every_container.rs"]
mod the_continuation_marker_attaches_one_block_in_every_container;
#[path = "../the_continuation_marker_s_column_gate_reaches_every_container.rs"]
mod the_continuation_marker_s_column_gate_reaches_every_container;
#[path = "../the_emitted_bytes_reparse_to_the_tree_they_came_from.rs"]
mod the_emitted_bytes_reparse_to_the_tree_they_came_from;
#[path = "../the_escape_search_reaches_the_minimal_form.rs"]
mod the_escape_search_reaches_the_minimal_form;
#[path = "../the_extension_list_in_the_docs_is_the_registry.rs"]
mod the_extension_list_in_the_docs_is_the_registry;
#[path = "../the_footnotes_placement_marker_is_picked.rs"]
mod the_footnotes_placement_marker_is_picked;
#[path = "../the_heading_index_is_built_in_document_order.rs"]
mod the_heading_index_is_built_in_document_order;
#[path = "../the_layout_path_answers_as_the_parser_does.rs"]
mod the_layout_path_answers_as_the_parser_does;
#[path = "../the_library_opens_no_file_without_the_fs_feature.rs"]
mod the_library_opens_no_file_without_the_fs_feature;
#[path = "../the_markdown_target_keeps_frontmatter.rs"]
mod the_markdown_target_keeps_frontmatter;
#[path = "../the_markdown_targets_escape_carriers_are_picked.rs"]
mod the_markdown_targets_escape_carriers_are_picked;
#[path = "../the_markdown_targets_escaping_narrows_on_the_line.rs"]
mod the_markdown_targets_escaping_narrows_on_the_line;
#[path = "../the_marker_column_tag_is_picked_per_document.rs"]
mod the_marker_column_tag_is_picked_per_document;
#[path = "../the_report_answers_to_the_published_schema.rs"]
mod the_report_answers_to_the_published_schema;
#[path = "../the_same_line_decides_what_an_import_keeps.rs"]
mod the_same_line_decides_what_an_import_keeps;
#[path = "../the_tree_taking_writer_keeps_a_frontmatter_block_as_written.rs"]
mod the_tree_taking_writer_keeps_a_frontmatter_block_as_written;
#[path = "../the_version_a_build_reports_is_the_one_that_shipped.rs"]
mod the_version_a_build_reports_is_the_one_that_shipped;
#[path = "../the_writer_invents_no_escape_the_re_parse_does_not_need.rs"]
mod the_writer_invents_no_escape_the_re_parse_does_not_need;
#[path = "../the_writer_keeps_a_continuation_marker.rs"]
mod the_writer_keeps_a_continuation_marker;
#[path = "../the_writers_own_output_parses_back_the_same.rs"]
mod the_writers_own_output_parses_back_the_same;
#[path = "../thematic_break.rs"]
mod thematic_break;
#[path = "../three_blank_lines_are_a_hard_list_boundary.rs"]
mod three_blank_lines_are_a_hard_list_boundary;
#[path = "../three_html_import_shapes_the_writer_and_importer_now_rule.rs"]
mod three_html_import_shapes_the_writer_and_importer_now_rule;
#[path = "../tight_item_fmt.rs"]
mod tight_item_fmt;
#[path = "../tight_item_trailing_text.rs"]
mod tight_item_trailing_text;
#[path = "../toc_heading_number_text.rs"]
mod toc_heading_number_text;
#[path = "../trailing_backslash_and_heading_cont.rs"]
mod trailing_backslash_and_heading_cont;
#[path = "../trailing_whitespace_is_dropped.rs"]
mod trailing_whitespace_is_dropped;
#[path = "../trailing_whitespace_lines_stay_placed.rs"]
mod trailing_whitespace_lines_stay_placed;
#[path = "../trojan_source_hardening.rs"]
mod trojan_source_hardening;
#[path = "../two_adjacent_code_spans_take_a_separator.rs"]
mod two_adjacent_code_spans_take_a_separator;
#[path = "../two_blank_lines_detach_a_caption.rs"]
mod two_blank_lines_detach_a_caption;
#[path = "../two_sibling_sub_lists_in_a_tight_item.rs"]
mod two_sibling_sub_lists_in_a_tight_item;
#[path = "../unknown_wire_property_is_refused.rs"]
mod unknown_wire_property_is_refused;
#[path = "../unquoted_attribute_values.rs"]
mod unquoted_attribute_values;
#[path = "../unresolved_footnote_targets.rs"]
mod unresolved_footnote_targets;
#[path = "../unresolved_reference_shape.rs"]
mod unresolved_reference_shape;
#[path = "../unterminated_comment_fence_opens_no_span.rs"]
mod unterminated_comment_fence_opens_no_span;
#[path = "../unterminated_fence_at_content_column.rs"]
mod unterminated_fence_at_content_column;
#[path = "../unterminated_fence_in_container.rs"]
mod unterminated_fence_in_container;
#[path = "../unused_definition_survives.rs"]
mod unused_definition_survives;
#[path = "../url_prefix_classification_follows_whatwg.rs"]
mod url_prefix_classification_follows_whatwg;
#[path = "../valid_colon_fence_in_a_quote.rs"]
mod valid_colon_fence_in_a_quote;
#[path = "../verbatim_sigils_stay_text.rs"]
mod verbatim_sigils_stay_text;
#[path = "../verse_registers_nothing.rs"]
mod verse_registers_nothing;
#[path = "../wire_fields_are_generated_from_the_schema.rs"]
mod wire_fields_are_generated_from_the_schema;
#[path = "../writer_catches_up.rs"]
mod writer_catches_up;
#[path = "../writer_footnote_body_column.rs"]
mod writer_footnote_body_column;
#[path = "../writer_keeps_invisible_characters.rs"]
mod writer_keeps_invisible_characters;
#[path = "../writer_keeps_tree_order.rs"]
mod writer_keeps_tree_order;
#[path = "../writer_private_use_sentinels.rs"]
mod writer_private_use_sentinels;

mod registry {
    use std::collections::BTreeSet;
    use std::path::Path;

    /// With `autotests = false`, a file nobody registers is never compiled, so
    /// its tests would pass by not existing.
    #[test]
    fn every_test_file_is_compiled() {
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let on_disk: BTreeSet<String> = std::fs::read_dir(&tests)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".rs"))
            .collect();
        let suite = include_str!("main.rs");
        let manifest = include_str!("../../Cargo.toml");
        let registered: BTreeSet<String> = suite
            .lines()
            .chain(manifest.lines())
            .filter_map(|line| {
                let line = line.trim();
                let rest = line
                    .strip_prefix("#[path = \"../")
                    .or_else(|| line.strip_prefix("path = \"tests/"))?;
                let name = rest.split('"').next()?;
                (!name.contains('/')).then(|| name.to_string())
            })
            .collect();
        let missing: Vec<_> = on_disk.difference(&registered).collect();
        assert!(
            missing.is_empty(),
            "not compiled by any test target; add `#[path = \"../NAME.rs\"] mod NAME;` \
             to tests/suite/main.rs: {missing:?}"
        );
        let stale: Vec<_> = registered.difference(&on_disk).collect();
        assert!(stale.is_empty(), "registered but gone: {stale:?}");
    }
}

#[path = "../nested_fence_ownership.rs"]
mod nested_fence_ownership;

#[path = "../container_boundaries.rs"]
mod container_boundaries;

#[path = "../emphasis_attribute_markers.rs"]
mod emphasis_attribute_markers;

#[path = "../markdown_empty_headings.rs"]
mod markdown_empty_headings;

#[path = "../a_quoted_container_fence_stores_no_lazy_claim.rs"]
mod a_quoted_container_fence_stores_no_lazy_claim;
#[path = "../djot_word_attributes.rs"]
mod djot_word_attributes;
#[path = "../markdown_fence_language.rs"]
mod markdown_fence_language;
