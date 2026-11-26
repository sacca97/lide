use rune::{col, icon, row, text, Cx, El, Tx};
use rune_kit::{root, Kit};

use crate::{kit, Workspace, CARD_GAP};

mod dialogs;
pub(crate) mod editor;
mod palette;
mod sidebar;
mod status;
pub(crate) mod terminal;

use dialogs::dialogs;
use editor::{
    build_panel, editor_area, external_change_bar, find_bar, hover_bar, tab_strip,
};
use palette::palette;
use sidebar::{sidebar, sidebar_resizer};
use status::status_bar;

/// Draw the whole window.
pub fn view(s: &Workspace, cx: &mut Cx<Workspace>) -> El {
    let k = kit(s.dark());
    let mut overlays = Vec::new();

    s.terminal_rect.set(cx.layout("terminal-area"));

    let mut main = row().w_full().grow(1.0).min_h(0.0);
    if s.settings.show_sidebar {
        main = main.child(sidebar(s, k)).child(sidebar_resizer(k));
    }
    main = main.child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .padding(CARD_GAP)
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .radius(10.0)
                    .clip_rounded(10.0)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.page)
                    .child(tab_strip(s, k))
                    .child(external_change_bar(s, k))
                    .child(find_bar(s, k))
                    .child(hover_bar(s, k))
                    .child(editor_area(s, k, cx))
                    .child(build_panel(s, k)),
            ),
    );

    let body = col()
        .w_full()
        .h_full()
        .fill(k.c.bg_deep)
        .child(main)
        .child(
            col()
                .w_full()
                .padding(CARD_GAP)
                .child(
                    col()
                        .w_full()
                        .radius(8.0)
                        .clip_rounded(8.0)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.bg_base)
                        .child(status_bar(s, k)),
                ),
        );

    // An invisible run of characters in the editor's font, measured from the
    // previous layout to find the width of one character.
    let body = body.child(
        text("M".repeat(64))
            .key("ruler")
            .font(crate::theme::mono_style(s.settings.font_size))
            .h(0.0)
            .opacity(0.001),
    );

    palette(s, k, &mut overlays);
    dialogs(s, k, &mut overlays);

    // A focused text field takes a paste itself; when none does, a terminal
    // is the thing that should receive it.
    root(body, overlays).on_paste(|s: &mut Workspace, text: &str, _tx: &mut Tx| {
        if let Some(terminal) = s.active_terminal_mut() {
            terminal.write(text);
        }
    })
}

pub(crate) fn empty_note(message: &str, k: &Kit) -> El {
    text(message.to_string())
        .font(k.ty.text_xsmall)
        .color(k.c.text_weak)
        .w_full()
        .px(8.0)
        .py(6.0)
}

pub(crate) fn icon_button(glyph: &'static rune::Icon, label: &str, k: &Kit) -> El {
    col()
        .key(format!("icon-button-{label}"))
        .label(label)
        .help(label)
        .size(22.0, 22.0)
        .radius(5.0)
        .center()
        .hover(k.c.surface_base)
        .child(icon(glyph, 14.0).color(k.c.text_weak))
}
