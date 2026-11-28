use std::path::{Path, PathBuf};

use rune::{col, el, icon, row, text, El, ScrollSpec};
use rune_kit::{Button, Kit, TextInput};

use crate::view::empty_note;
use crate::{
    clamp_sidebar_width, tree::TreeNode, SidebarMode, Workspace, CARD_GAP,
    SIDEBAR_RAIL_WIDTH,
};

pub(crate) fn sidebar(s: &Workspace, k: &Kit) -> El {
    let mut activity = col().w(SIDEBAR_RAIL_WIDTH).h_full().fill(k.c.bg_deep);
    for (glyph, mode, name) in [
        (&crate::icons::FILES, SidebarMode::Files, "Files"),
        (&crate::icons::SEARCH, SidebarMode::Search, "Search"),
        (&crate::icons::PROBLEMS, SidebarMode::Problems, "Problems"),
        (&crate::icons::PLACES, SidebarMode::Places, "Places"),
    ] {
        activity = activity.child(activity_item(glyph, name, mode, s.sidebar == mode, k));
    }

    // The rail stays when the panel is collapsed, so there is always a
    // visible way back. Clicking a mode also reopens the panel.
    let (glyph, label) = if s.settings.show_sidebar {
        (&crate::icons::CHEVRON_LEFT, "Collapse sidebar")
    } else {
        (&crate::icons::CHEVRON_RIGHT, "Expand sidebar")
    };
    activity = activity.child(el().grow(1.0)).child(
        row().w_full().h(38.0).px(4.0).center_y().child(
            col()
                .key("sidebar-toggle")
                .label(label)
                .grow(1.0)
                .h(32.0)
                .radius(6.0)
                .center()
                .child(icon(glyph, 16.0).color(k.c.text_weak))
                .hover(k.c.surface_base)
                .on_press(|s: &mut Workspace| s.toggle_sidebar()),
        ),
    );

    let body = match s.sidebar {
        SidebarMode::Files => file_tree(s, k),
        SidebarMode::Search => search_panel(s, k),
        SidebarMode::Problems => problems_panel(s, k),
        SidebarMode::Places => places_panel(s, k),
    };
    let title = match s.sidebar {
        SidebarMode::Files => "Explorer",
        SidebarMode::Search => "Search",
        SidebarMode::Problems => "Problems",
        SidebarMode::Places => "Places",
    };

    let panel = col()
        .w((s.settings.sidebar_width - SIDEBAR_RAIL_WIDTH - CARD_GAP * 2.0).max(120.0))
        .grow(1.0)
        .min_h(0.0)
        .radius(8.0)
        .clip_rounded(8.0)
        .border(1.0, k.c.border_base)
        .fill(k.c.bg_base)
        .child(
            row().w_full().h(32.0).px(12.0).center_y().child(
                text(title.to_string())
                    .font(k.ty.small_medium)
                    .color(k.c.text_muted),
            ),
        )
        .child(body);

    row()
        .h_full()
        .child(activity)
        .child(col().padding(CARD_GAP).h_full().child(panel))
}

/// The draggable edge between the sidebar and the editor.
pub(crate) fn sidebar_resizer(k: &Kit) -> El {
    col()
        .key("sidebar-resizer")
        .label("Resize sidebar")
        .w(5.0)
        .h_full()
        .center()
        .fill(k.c.bg_deep)
        .cursor(rune::Cursor::ResizeColumn)
        .hit()
        .hover(k.c.surface_hover)
        .child(
            el()
                .w(2.0)
                .h(36.0)
                .radius(1.0)
                .fill(k.c.border_strong),
        )
        .on_drag(|s: &mut Workspace, drag: &mut rune::Drag, _tx: &mut rune::Tx| {
            if drag.phase == rune::Phase::Up {
                // The release point is relative to the capture origin and is
                // not the last Move position; do not apply it.
                s.sidebar_drag_start = None;
                s.settings.save();
                return;
            }
            if s.sidebar_drag_start.is_none() {
                s.sidebar_drag_start = Some(s.settings.sidebar_width);
            }
            if let Some(start) = s.sidebar_drag_start {
                s.settings.sidebar_width =
                    clamp_sidebar_width(start + drag.x - drag.start_x);
            }
        })
}

/// One entry of the narrow activity bar down the left edge.
fn activity_item(glyph: &'static rune::Icon, label: &str, mode: SidebarMode, active: bool, k: &Kit) -> El {
    row()
        .key(format!("activity-{label}"))
        .label(label)
        .w_full()
        .h(38.0)
        .px(4.0)
        .center_y()
        .child(
            el()
                .w(2.0)
                .h(20.0)
                .radius(1.0)
                .fill(if active {
                    k.c.text_accent
                } else {
                    k.c.text_accent.alpha(0.0)
                }),
        )
        .child(
            col()
                .grow(1.0)
                .h(32.0)
                .radius(6.0)
                .center()
                .hover(k.c.surface_base)
                .on_press(move |s: &mut Workspace| {
                    s.sidebar = mode;
                    s.settings.show_sidebar = true;
                })
                .child(icon(glyph, 18.0).color(if active {
                    k.c.text_base
                } else {
                    k.c.text_weak
                })),
        )
}

fn places_panel(s: &Workspace, k: &Kit) -> El {
    let mut list = col().w_full().gap(1.0);

    if s.references.is_empty() {
        list = list.child(empty_note(
            "No references. Put the cursor on a symbol and use Go to references.",
            k,
        ));
    }

    for (path, line, column) in s.references.iter().take(300) {
        let name = path
            .strip_prefix(s.root.as_deref().unwrap_or(std::path::Path::new("")))
            .unwrap_or(path)
            .display()
            .to_string();
        let path = path.clone();
        let line = *line;
        let column = *column;

        list = list.child(
            row()
                .key(format!("place-{name}-{line}-{column}"))
                .w_full()
                .px(8.0)
                .py(3.0)
                .radius(5.0)
                .hover(k.c.hover)
                .on_press(move |s: &mut Workspace| {
                    s.open_match(path.clone(), line, column);
                })
                .child(
                    text(format!("{name}:{line}"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_base)
                        .min_w(0.0)
                        .truncate(),
                ),
        );
    }

    col()
        .key("places")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .px(4.0)
        .child(list)
}

fn problems_panel(s: &Workspace, k: &Kit) -> El {
    let all = s.servers.all_diagnostics();
    let mut list = col().w_full().gap(1.0);

    if all.is_empty() {
        list = list.child(empty_note("No problems reported", k));
    }

    for (index, diagnostic) in all.iter().take(300).enumerate() {
        let (colour, label) = match diagnostic.severity {
            lide_lsp::Severity::Error => (k.c.danger.fg, "error"),
            lide_lsp::Severity::Warning => (k.c.warning.fg, "warning"),
            lide_lsp::Severity::Information => (k.c.text_accent, "info"),
            lide_lsp::Severity::Hint => (k.c.text_weak, "hint"),
        };
        let name = diagnostic
            .path
            .strip_prefix(s.root.as_deref().unwrap_or(std::path::Path::new("")))
            .unwrap_or(&diagnostic.path)
            .display()
            .to_string();
        let path = diagnostic.path.clone();
        let line = diagnostic.start.line + 1;
        let column = diagnostic.start.character as usize;

        list = list.child(
            col()
                .key(format!(
                    "problem-{name}-{line}-{}-{index}",
                    diagnostic.start.character
                ))
                .w_full()
                .px(8.0)
                .py(3.0)
                .radius(5.0)
                .gap(1.0)
                .hover(k.c.hover)
                .on_press(move |s: &mut Workspace| {
                    s.open_match(path.clone(), line, column);
                })
                .child(
                    text(format!("{name}:{line}  {label}"))
                        .font(k.ty.small_medium)
                        .color(colour)
                        .w_full()
                        .min_w(0.0)
                        .truncate(),
                )
                .child(
                    text(diagnostic.message.clone())
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .w_full(),
                ),
        );
    }

    col()
        .key("problems")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .px(4.0)
        .child(list)
}

fn file_tree(s: &Workspace, k: &Kit) -> El {
    let active = s
        .active_document()
        .and_then(|document| document.path())
        .map(|path| path.to_path_buf());
    let mut tree = col().w_full().gap(0.0);
    if s.root.is_none() {
        tree = tree.child(empty_note("No folder open", k));
    }
    for node in s.tree.nodes() {
        tree = tree.child(node_row(node, k, 0, active.as_deref(), s.collapsed_paths.clone()));
    }

    col()
        .key("file-tree")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .px(4.0)
        .py(2.0)
        .gap(0.0)
        .child(tree)
}

fn search_panel(s: &Workspace, k: &Kit) -> El {
    let mut results = col().w_full().gap(1.0);

    match &s.search {
        None => {
            results = results.child(empty_note("Type to search the workspace", k));
        }
        Some(search) => {
            if let Some(error) = &search.error {
                results = results.child(text(error.clone()).font(k.ty.small).color(k.c.danger.fg));
            }
            for found in search.matches.iter().take(300) {
                let name = found
                    .path
                    .strip_prefix(s.root.as_deref().unwrap_or(std::path::Path::new("")))
                    .unwrap_or(&found.path)
                    .display()
                    .to_string();
                let path = found.path.clone();
                let line = found.line;
                let column = found.ranges.first().map(|range| range.start).unwrap_or(0);

                results = results.child(
                    col()
                        .key(format!("match-{name}-{line}-{column}"))
                        .w_full()
                        .px(8.0)
                        .py(3.0)
                        .radius(5.0)
                        .gap(1.0)
                        .hover(k.c.hover)
                        .on_press(move |s: &mut Workspace| {
                            s.open_match(path.clone(), line, column);
                        })
                        .child(
                            text(format!("{name}:{line}"))
                                .font(k.ty.small_medium)
                                .color(k.c.text_base)
                                .w_full()
                                .min_w(0.0)
                                .truncate(),
                        )
                        .child(
                            text(squash_whitespace(&found.text))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted)
                                .w_full()
                                .min_w(0.0)
                                .truncate(),
                        ),
                );
            }
        }
    }

    let summary = match &s.search {
        None => String::new(),
        Some(search) if search.running => format!("{} matches so far…", search.matches.len()),
        Some(search) => format!(
            "{} matches in {} files",
            search.matches.len(),
            search.file_count()
        ),
    };

    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(8.0)
        .child(
            col()
                .w_full()
                .px(8.0)
                .gap(6.0)
                .child(
                    TextInput::new(&s.search_query)
                        .placeholder("Search")
                        .width(None)
                        .on_edit(|s: &mut Workspace| &mut s.search_query)
                        .on_change(|s: &mut Workspace, _| s.run_search())
                        .build(k),
                )
                .child(
                    row()
                        .w_full()
                        .gap(4.0)
                        .center_y()
                        .child(
                            toggle_chip("Regex", ".*", s.search_regex, k).on_press(
                                |s: &mut Workspace| {
                                    s.search_regex = !s.search_regex;
                                    s.run_search();
                                },
                            ),
                        )
                        .child(
                            toggle_chip("Match case", "Aa", s.search_case_sensitive, k).on_press(
                                |s: &mut Workspace| {
                                    s.search_case_sensitive = !s.search_case_sensitive;
                                    s.run_search();
                                },
                            ),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(summary)
                                .font(k.ty.small)
                                .color(k.c.text_weak)
                                .min_w(0.0)
                                .truncate(),
                        ),
                ),
        )
        .child(
            row()
                .w_full()
                .px(8.0)
                .gap(6.0)
                .center_y()
                .child(
                    col().grow(1.0).min_w(0.0).child(
                        TextInput::new(&s.search_replacement)
                            .placeholder("Replace with")
                            .width(None)
                            .on_edit(|s: &mut Workspace| &mut s.search_replacement)
                            .build(k),
                    ),
                )
                .child(
                    Button::new("All")
                        .help("Replace all matches")
                        .on_press(|s: &mut Workspace| s.begin_replace_all())
                        .build(k),
                ),
        )
        .child(
            col()
                .key("search-results")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .px(4.0)
                .child(results),
        )
}

/// A small on/off switch beside a search box: a bordered chip when on.
fn toggle_chip(label: &str, face: &str, on: bool, k: &Kit) -> El {
    row()
        .key(format!("toggle-{label}"))
        .label(label)
        .h(22.0)
        .px(6.0)
        .center_y()
        .radius(5.0)
        .border(
            1.0,
            if on {
                k.c.border_strong
            } else {
                k.c.border_base.alpha(0.0)
            },
        )
        .fill(if on { k.c.surface_raised_hover } else { k.c.bg_base })
        .hover(if on { k.c.surface_raised_hover } else { k.c.hover })
        .child(
            text(face.to_string())
                .font(k.ty.small_medium)
                .color(if on { k.c.text_base } else { k.c.text_weak }),
        )
}

/// A line of source collapsed to one tidy line: tabs and runs of spaces become
/// a single space and the indentation goes, so a result reads as a snippet.
fn squash_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn node_row(
    node: &TreeNode,
    k: &Kit,
    depth: usize,
    active: Option<&Path>,
    collapsed: std::collections::HashSet<PathBuf>,
) -> El {
    let expanded = !collapsed.contains(&node.path);
    let current = !node.is_dir && active == Some(node.path.as_path());
    let name_colour = if current || node.is_dir {
        k.c.text_base
    } else {
        k.c.text_muted
    };

    // Every row has the same three columns (chevron, icon, name), so a file
    // lines up with the folders beside it and the indent is the only thing
    // that varies with depth.
    let chevron = if node.is_dir {
        icon(
            if expanded {
                &crate::icons::CHEVRON_DOWN
            } else {
                &crate::icons::CHEVRON_RIGHT
            },
            14.0,
        )
        .color(k.c.text_weak)
    } else {
        el().size(14.0, 14.0)
    };
    let kind: &'static rune::Icon = if node.is_dir {
        if expanded {
            &crate::icons::FOLDER_OPEN
        } else {
            &crate::icons::FOLDER
        }
    } else {
        crate::icons::for_file(&node.name)
    };

    let mut row = row()
        .key(format!("node-{}", node.path.display()))
        .w_full()
        .h(24.0)
        .pl(4.0 + depth as f32 * 12.0)
        .pr(6.0)
        .gap(4.0)
        .center_y()
        .radius(5.0)
        .child(chevron)
        .child(icon(kind, 14.0).color(if current {
            k.c.text_base
        } else {
            k.c.text_weak
        }))
        .child(
            text(node.name.clone())
                .font(k.ty.text_xsmall)
                .color(name_colour)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        );

    if current {
        row = row.fill(k.c.surface_base);
    }

    let path = node.path.clone();
    if node.is_dir {
        row = row.on_press(move |s: &mut Workspace| s.toggle_directory(&path));
    } else {
        row = row.on_press(move |s: &mut Workspace| {
            if let Err(error) = s.open_path(&path) {
                s.set_notice(error);
            }
        });
    }
    row = row.hover(k.c.hover);

    let mut out = col().w_full().child(row);
    if node.is_dir && expanded {
        for child in &node.children {
            out = out.child(node_row(child, k, depth + 1, active, collapsed.clone()));
        }
    }
    out
}
