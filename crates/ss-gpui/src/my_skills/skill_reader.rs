//! Floating reader for one `SKILL.md`.
//!
//! The detail column keeps the summary and a button. This dialog renders the
//! file as Markdown. In Chinese, a button on the file label translates this
//! opening only: each English paragraph gains a cached line under it. The
//! SKILL.md switch in Settings still turns that on for every opening, and
//! the button does not write the switch. Closing the dialog returns to the
//! column.
//!
//! The kit dialog slides for 250ms and rebuilds its layer every frame. A
//! child view re-renders whenever its window bounds move, so the file stays
//! out of the tree until that slide has stopped.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::dialog::DialogTitle;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::infra::error::AppError;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::spawn_domain;
use crate::theme::palette;

/// Kit dialog entrance is 250ms. Stay empty a little longer so the last
/// sliding frame still lays out the shell, not the file.
const ENTRANCE: Duration = Duration::from_millis(320);

/// Kit dialog `max_h` keeps `spacing.lg` clear of each edge.
const DIALOG_MARGIN: f32 = 32.0;

/// Padding and border outside the measured child. Matches `DialogChrome::Padded`.
const DIALOG_CHROME: f32 = 34.0;

/// The dialog subtracts the window title inset before that max height.
/// Leave this much so the scroller stays inside the card on a titled window.
const WINDOW_INSET: f32 = 48.0;

enum Origin {
    Installed(String),
    Provided { title: String, markdown: String },
}

enum Phase {
    Loading,
    Ready(String),
    Failed(String),
}

pub(crate) struct SkillReader {
    origin: Origin,
    phase: Phase,
    generation: u64,
    revealed: bool,
    entrance_armed: bool,
    /// Show translations for this opening. Starts from the SKILL.md setting.
    show_translation: bool,
    translation_chosen: bool,
}

impl SkillReader {
    fn installed(name: String) -> Self {
        Self {
            origin: Origin::Installed(name),
            phase: Phase::Loading,
            generation: 0,
            revealed: false,
            entrance_armed: false,
            show_translation: false,
            translation_chosen: false,
        }
    }

    fn provided(title: String, markdown: String) -> Self {
        Self {
            origin: Origin::Provided { title, markdown },
            phase: Phase::Loading,
            generation: 0,
            revealed: false,
            entrance_armed: false,
            show_translation: false,
            translation_chosen: false,
        }
    }

    fn title(&self) -> String {
        match &self.origin {
            Origin::Installed(name) => name.clone(),
            Origin::Provided { title, .. } => title.clone(),
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.translation_chosen {
            self.show_translation =
                crate::translation::enabled(crate::translation::Surface::SkillMd);
            self.translation_chosen = true;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        match &self.origin {
            Origin::Provided { markdown, .. } => {
                self.phase = Phase::Ready(markdown.clone());
                cx.notify();
            }
            Origin::Installed(name) => {
                let name = name.clone();
                let view = cx.entity();
                spawn_domain(
                    &view,
                    cx,
                    async move {
                        match tokio::task::spawn_blocking(move || ss_skills::content::read(&name))
                            .await
                        {
                            Ok(result) => result,
                            Err(error) => Err(AppError::Other(error.to_string())),
                        }
                    },
                    move |this, _, result| {
                        if this.generation != generation {
                            return;
                        }
                        this.phase = match result {
                            Ok(content) => Phase::Ready(content.content),
                            Err(error) => Phase::Failed(error.to_string()),
                        };
                    },
                );
            }
        }
        self.arm_entrance(cx);
    }

    fn arm_entrance(&mut self, cx: &mut Context<Self>) {
        if self.revealed || self.entrance_armed {
            return;
        }
        if cx.reduce_motion() {
            self.revealed = true;
            return;
        }
        self.entrance_armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ENTRANCE).await;
            let _ = this.update(cx, |this, cx| {
                this.revealed = true;
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn open_skill_reader(name: String, window: &mut Window, cx: &mut App) {
    open_reader(cx.new(|_| SkillReader::installed(name)), window, cx);
}

/// Marketplace detail already has the markdown. It does not read the disk.
pub(crate) fn open_skill_markdown(
    title: String,
    markdown: String,
    window: &mut Window,
    cx: &mut App,
) {
    open_reader(
        cx.new(|_| SkillReader::provided(title, markdown)),
        window,
        cx,
    );
}

fn reader_shell_height(viewport_h: f32) -> f32 {
    let fit = viewport_h - DIALOG_MARGIN - DIALOG_CHROME - WINDOW_INSET;
    fit.clamp(400.0, 840.0)
}

fn open_reader(reader: Entity<SkillReader>, window: &mut Window, cx: &mut App) {
    reader.update(cx, |this, cx| this.start(cx));
    let shown = reader.clone();
    crate::chrome::open_centered(
        window,
        cx,
        720.0,
        crate::chrome::DialogChrome::Padded,
        move |dialog, frame, window, _| {
            let viewport = window.viewport_size();
            let width = (viewport.width.as_f32() - 80.0).clamp(720.0, 1040.0);
            let shell = reader_shell_height(viewport.height.as_f32());
            frame.seed(shell);
            // The kit backdrop closes on a press outside the card.
            dialog
                .overlay(true)
                .overlay_closable(true)
                .w(px(width))
                .child(
                    frame.measure(
                        div()
                            .relative()
                            .w_full()
                            .h(px(shell))
                            .overflow_hidden()
                            .child(crate::chrome::replay_view(shown.clone().into())),
                    ),
                )
        },
    );
}

impl crate::translation::TranslationHost for SkillReader {}

impl Render for SkillReader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let translate = self.show_translation;
        if self.revealed && translate {
            if let Phase::Ready(text) = &self.phase {
                crate::translation::schedule_when(
                    &cx.entity(),
                    split_markdown(text)
                        .into_iter()
                        .filter_map(|block| match block {
                            MdBlock::Prose(text) => Some(text),
                            MdBlock::Code(_) => None,
                        }),
                    true,
                    cx,
                );
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(DialogTitle::new().pr_6().child(self.title()))
            .child(self.label_row(cx))
            .child(self.body(translate, cx))
    }
}

impl SkillReader {
    fn label_row(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .min_w_0()
            .pr_6()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().fg_muted))
                    .child("SKILL.md"),
            );
        if let Some(button) = self.translate_button(cx) {
            row = row.child(button);
        }
        row
    }

    /// Icon-only translate button on the SKILL.md label row: the same glyph,
    /// the same laser sweep, and the same opening-only choice as the detail
    /// column's description button. Chinese prose is not something the
    /// target would translate, so no button.
    fn translate_button(&self, cx: &mut Context<Self>) -> Option<crate::chrome::MotionDiv> {
        let Phase::Ready(text) = &self.phase else {
            return None;
        };
        if text.trim().is_empty() || !offers_translation(text) {
            return None;
        }
        let showing = self.show_translation;
        let pending = showing && translation_pending(text);
        let tip = if showing && !pending {
            crate::i18n::t("detailPanel.showOriginal")
        } else {
            crate::i18n::t("detailPanel.translate")
        };
        let view = cx.entity().downgrade();
        Some(
            div()
                .id("skill-md-translate")
                .p(px(3.0))
                .rounded_md()
                .flex_shrink_0()
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    crate::chrome::tooltip(tip.to_string()).build(window, cx)
                })
                .child(crate::chrome::icon_sweep(
                    "skill-md-translate",
                    IconName::Languages,
                    14.0,
                    palette().fg_muted,
                    pending,
                ))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.show_translation = !this.show_translation;
                        cx.notify();
                    });
                })
                .interaction_spring(
                    "skill-md-translate",
                    true,
                    MotionPaint::new().fg(rgb(palette().fg_muted)),
                    MotionPaint::new()
                        .fg(rgb(palette().fg))
                        .bg(rgb(palette().card_hover)),
                )
                .debug_selector(|| "skill-md-translate".into()),
        )
    }

    fn body(&mut self, translate: bool, cx: &mut Context<Self>) -> AnyElement {
        if !self.revealed {
            return scroll_port(false, div());
        }
        match &self.phase {
            Phase::Loading => scroll_port(
                false,
                muted_line(crate::i18n::t("detailPanel.reading").to_string()),
            ),
            Phase::Failed(error) => self.failure(error.clone(), cx),
            Phase::Ready(text) if text.trim().is_empty() => scroll_port(
                false,
                muted_line(crate::i18n::t("detailPanel.emptySkillMd").to_string()),
            ),
            Phase::Ready(text) => scroll_port(true, file_column(text, translate)),
        }
    }

    fn failure(&self, error: String, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(palette().danger))
                    .child(crate::i18n::t("detailPanel.readFailed").to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(error),
            )
            .child(
                div()
                    .id("skill-md-retry")
                    .self_start()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette().border))
                    .bg(rgb(palette().card))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().fg))
                    .cursor_pointer()
                    .child(crate::i18n::t("common.retry").to_string())
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.phase = Phase::Loading;
                            this.start(cx);
                        });
                    })
                    .interaction_spring(
                        "skill-md-retry",
                        true,
                        MotionPaint::new().bg(rgb(palette().card)),
                        MotionPaint::new().bg(rgb(palette().card_hover)),
                    ),
            )
            .into_any_element()
    }
}

fn muted_line(text: String) -> Div {
    div()
        .text_sm()
        .text_color(rgb(palette().fg_muted))
        .child(text)
}

fn scroll_port(ready: bool, child: impl IntoElement) -> AnyElement {
    // The leftover height, not a budgeted one. The title and the file label
    // are text, so any constant here goes stale and leaves a dead band under
    // the card. `flex_1` hands the rest of the fixed shell to this frame,
    // which makes the scroller's `size_full()` a definite height.
    // The selector stays on this frame: `overflow_y_scrollbar` moves a
    // selector on the scrolled element onto the content, which is as tall
    // as the file.
    let mut frame = div().flex_1().min_h_0().w_full().min_w_0();
    if ready {
        frame = frame.debug_selector(|| "skill-md-body".into());
    }
    frame
        .child(
            div()
                .id("skill-md-reader")
                .size_full()
                .min_w_0()
                .overflow_y_scrollbar()
                .p_3()
                .rounded_lg()
                .bg(rgb(palette().card))
                .border_1()
                .border_color(rgb(palette().border))
                .child(child),
        )
        .into_any_element()
}

fn file_column(text: &str, translate: bool) -> Div {
    let mut column = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_3()
        .debug_selector(|| "skill-md-column".into());
    if translate {
        if let Some(hint) = crate::translation::blocked_hint() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(hint),
            );
        }
    }
    if translate {
        for (index, block) in split_markdown(text).into_iter().enumerate() {
            match block {
                MdBlock::Code(markdown) => {
                    column =
                        column.child(TextView::markdown(format!("skill-md-{index}"), markdown));
                }
                MdBlock::Prose(markdown) => {
                    let translated = crate::translation::display_when(&markdown, true);
                    column = column.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(TextView::markdown(
                                format!("skill-md-{index}"),
                                markdown.clone(),
                            ))
                            .when(translated != markdown, |block| {
                                block.child(crate::translation::paint_reader(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_sm()
                                        .whitespace_normal()
                                        .debug_selector(|| "skill-md-translation".into())
                                        .child(translated),
                                ))
                            }),
                    );
                }
            }
        }
        return column;
    }
    let (front, body) = peel_frontmatter(text);
    if let Some(front) = front {
        column = column.child(TextView::markdown("skill-md-front", front));
    }
    if !body.trim().is_empty() {
        column = column.child(TextView::markdown("skill-md-rest", body));
    }
    column
}

fn offers_translation(text: &str) -> bool {
    let target = ss_core::translation::active_target();
    split_markdown(text).into_iter().any(|block| {
        matches!(block, MdBlock::Prose(prose) if ss_core::translation::needs_translation(&prose, target))
    })
}

fn translation_pending(text: &str) -> bool {
    if crate::translation::blocked_hint().is_some() {
        return false;
    }
    split_markdown(text).into_iter().any(
        |block| matches!(block, MdBlock::Prose(prose) if !crate::translation::resolved(&prose)),
    )
}

enum MdBlock {
    Code(String),
    Prose(String),
}

fn peel_frontmatter(text: &str) -> (Option<String>, String) {
    let text = text.replace("\r\n", "\n");
    if let Some(after) = text.strip_prefix("---\n")
        && let Some(end) = after.find("\n---")
    {
        let yaml = after[..end].trim_end();
        let body = after[end + "\n---".len()..]
            .strip_prefix('\n')
            .unwrap_or("")
            .to_string();
        return (Some(format!("```yaml\n{yaml}\n```")), body);
    }
    (None, text)
}

fn split_markdown(text: &str) -> Vec<MdBlock> {
    let (front, body) = peel_frontmatter(text);
    let mut blocks = Vec::new();
    if let Some(front) = front {
        blocks.push(MdBlock::Code(front));
    }
    let mut prose = String::new();
    let mut code = String::new();
    let mut in_code = false;
    for line in body.lines() {
        if in_code {
            code.push_str(line);
            code.push('\n');
            if line.trim_start().starts_with("```") {
                in_code = false;
                blocks.push(MdBlock::Code(
                    std::mem::take(&mut code).trim_end().to_string(),
                ));
            }
            continue;
        }
        if line.trim_start().starts_with("```") {
            flush_prose(&mut prose, &mut blocks);
            in_code = true;
            code.push_str(line);
            code.push('\n');
            continue;
        }
        if line.trim().is_empty() {
            flush_prose(&mut prose, &mut blocks);
            continue;
        }
        if !prose.is_empty() {
            prose.push('\n');
        }
        prose.push_str(line);
    }
    if in_code {
        blocks.push(MdBlock::Code(code.trim_end().to_string()));
    } else {
        flush_prose(&mut prose, &mut blocks);
    }
    blocks
}

fn flush_prose(prose: &mut String, blocks: &mut Vec<MdBlock>) {
    let text = prose.trim();
    if !text.is_empty() {
        blocks.push(MdBlock::Prose(text.to_string()));
    }
    prose.clear();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui_kit::component::Root;
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::{
        AppContext, Context, IntoElement, Render, ScrollDelta, ScrollWheelEvent, Styled, Window,
        div, point, px, size,
    };

    use super::super::test_support::IsolatedDataDir;
    use super::{
        DIALOG_CHROME, DIALOG_MARGIN, MdBlock, WINDOW_INSET, open_skill_markdown,
        reader_shell_height, split_markdown,
    };

    #[test]
    fn frontmatter_and_code_stay_out_of_prose() {
        let text = "---\nname: demo\n---\n\nHello world.\n\n```\ncode\n```\n\nSecond paragraph.\n";
        let blocks = split_markdown(text);
        assert!(matches!(&blocks[0], MdBlock::Code(code) if code.contains("name: demo")));
        assert!(matches!(&blocks[1], MdBlock::Prose(text) if text == "Hello world."));
        assert!(matches!(&blocks[2], MdBlock::Code(code) if code.contains("code")));
        assert!(matches!(&blocks[3], MdBlock::Prose(text) if text == "Second paragraph."));
    }

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }

    fn window(
        cx: &mut gpui_kit::TestAppContext,
        reduce_motion: bool,
    ) -> &mut gpui_kit::VisualTestContext {
        crate::init_test(cx);
        cx.update(|cx| cx.set_reduce_motion(reduce_motion));
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let host = cx.new(|_| Blank);
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx
    }

    fn paint(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui_kit::test]
    fn markdown_stays_unmounted_until_the_slide_stops(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, false);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "你好。\n".into(), window, cx);
        });
        paint(cx);
        paint(cx);
        assert!(
            cx.debug_bounds("skill-md-body").is_none(),
            "the file laid out while the dialog was still sliding"
        );

        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        paint(cx);
        assert!(
            cx.debug_bounds("skill-md-body").is_some(),
            "the file never appeared after the entrance"
        );
    }

    #[gpui_kit::test]
    fn reduce_motion_mounts_the_file_on_the_first_paint(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "你好。\n".into(), window, cx);
        });
        paint(cx);
        assert!(cx.debug_bounds("skill-md-body").is_some());
    }

    #[gpui_kit::test]
    fn the_card_stays_put_after_the_first_frame(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "你好。\n".into(), window, cx);
        });
        paint(cx);
        let first = cx.debug_bounds("dialog-0").expect("dialog").origin.y;
        paint(cx);
        let second = cx.debug_bounds("dialog-0").expect("dialog").origin.y;
        assert_eq!(first, second, "centering jumped after the first frame");
    }

    #[gpui_kit::test]
    fn chinese_prose_hides_the_translate_button(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "构建桌面应用。\n".into(), window, cx);
        });
        paint(cx);
        assert!(
            cx.debug_bounds("skill-md-translate").is_none(),
            "already-Chinese prose still offered translate"
        );
        assert!(
            cx.debug_bounds("skill-md-translation").is_none(),
            "a translation line was painted from the reader"
        );
    }

    #[gpui_kit::test]
    fn english_ui_offers_the_default_chinese_translation(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("en");
        let _dir = IsolatedDataDir::new();
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "Hello world.\n".into(), window, cx);
        });
        paint(cx);
        assert!(cx.debug_bounds("skill-md-translate").is_some());
        assert!(cx.debug_bounds("skill-md-translation").is_none());
    }

    #[gpui_kit::test]
    fn the_translate_button_shows_and_hides_a_cached_line(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        ss_core::translation::remember("Hello world.", "zh-CN", "你好。");
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "Hello world.\n".into(), window, cx);
        });
        paint(cx);
        let button = cx
            .debug_bounds("skill-md-translate")
            .expect("translate button");
        assert!(
            cx.debug_bounds("skill-md-translation").is_none(),
            "the line appeared before the button"
        );
        let dialog = cx.debug_bounds("dialog-0").expect("dialog");
        let port = cx.debug_bounds("skill-md-body").expect("body");
        assert!(
            port.origin.y + port.size.height <= dialog.origin.y + dialog.size.height + px(1.),
            "the scroller sticks out of the card: port {port:?} dialog {dialog:?}"
        );
        click(cx, button);
        paint(cx);
        assert!(
            cx.debug_bounds("skill-md-translation").is_some(),
            "the cached line did not appear"
        );
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        let button = cx
            .debug_bounds("skill-md-translate")
            .expect("the button left after translating");
        click(cx, button);
        paint(cx);
        assert!(
            cx.debug_bounds("skill-md-translation").is_none(),
            "Original left the translation line up"
        );
    }

    #[gpui_kit::test]
    fn the_skill_md_setting_translates_before_a_click(cx: &mut gpui_kit::TestAppContext) {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let _dir = IsolatedDataDir::new();
        let mut config = ss_core::translation::TranslationConfig::default();
        config.translate_skill_md = true;
        ss_core::translation::save_config(&config).unwrap();
        ss_core::translation::remember("Hello world.", "zh-CN", "你好。");
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "Hello world.\n".into(), window, cx);
        });
        paint(cx);
        assert!(cx.debug_bounds("skill-md-translate").is_some());
        assert!(
            cx.debug_bounds("skill-md-translation").is_some(),
            "the SKILL.md switch did not translate this opening"
        );
    }

    fn click(cx: &mut gpui_kit::VisualTestContext, bounds: gpui_kit::Bounds<gpui_kit::Pixels>) {
        cx.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            ),
            Default::default(),
        );
    }

    #[gpui_kit::test]
    fn a_press_outside_closes_and_a_press_on_the_card_does_not(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, true);
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), "你好。\n".into(), window, cx);
        });
        paint(cx);
        paint(cx);
        let bounds = cx.debug_bounds("dialog-0").expect("dialog");
        assert!(
            bounds.origin.x > px(16.),
            "the card covers the left edge, so there is no outside press: {bounds:?}"
        );
        let center = point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        );
        cx.simulate_click(center, Default::default());
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

        cx.simulate_click(point(px(4.), bounds.origin.y + px(48.)), Default::default());
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    #[test]
    fn the_reader_card_fits_in_the_dialog_max() {
        for viewport in [700.0, 900.0, 1400.0] {
            let shell = reader_shell_height(viewport);
            let card = shell + DIALOG_CHROME;
            let max_card = viewport - DIALOG_MARGIN - WINDOW_INSET;
            assert!(
                card <= max_card + 0.5,
                "viewport {viewport}: card {card} exceeds {max_card}"
            );
        }
    }

    #[gpui_kit::test]
    fn the_file_fills_the_card_down_to_its_padding(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, true);
        let body = (0..40)
            .map(|index| format!("第 {index} 段说明这一步，句子足够长，会在卡片里换行。"))
            .collect::<Vec<_>>()
            .join("\n\n");
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), body, window, cx);
        });
        paint(cx);
        let dialog = cx.debug_bounds("dialog-0").expect("dialog");
        let port = cx.debug_bounds("skill-md-body").expect("body");
        let gap = dialog.origin.y + dialog.size.height - (port.origin.y + port.size.height);
        assert_eq!(
            gap,
            px(DIALOG_CHROME / 2.),
            "the card leaves a dead band under the file: port {port:?} dialog {dialog:?}"
        );
    }

    #[gpui_kit::test]
    fn the_wheel_moves_a_long_file(cx: &mut gpui_kit::TestAppContext) {
        let cx = window(cx, true);
        let body = (0..40)
            .map(|index| format!("第 {index} 段说明这一步，句子足够长，会在卡片里换行。"))
            .collect::<Vec<_>>()
            .join("\n\n");
        cx.update(|window, cx| {
            open_skill_markdown("demo".into(), body, window, cx);
        });
        paint(cx);
        let port = cx.debug_bounds("skill-md-body").expect("body");
        let dialog = cx.debug_bounds("dialog-0").expect("dialog");
        assert!(
            port.origin.y + port.size.height <= dialog.origin.y + dialog.size.height + px(1.),
            "the scroller sticks out of the card: port {port:?} dialog {dialog:?}"
        );
        let before = cx.debug_bounds("skill-md-column").expect("column").origin.y;
        cx.simulate_event(ScrollWheelEvent {
            position: point(port.origin.x + px(32.), port.origin.y + px(32.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-280.))),
            ..Default::default()
        });
        paint(cx);
        let after = cx.debug_bounds("skill-md-column").expect("column").origin.y;
        assert!(
            after < before,
            "wheel did not scroll the file: before {before:?} after {after:?}"
        );
    }
}
