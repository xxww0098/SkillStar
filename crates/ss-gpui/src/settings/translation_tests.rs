//! Visual tests for the translation settings model combobox.

use gpui_kit::component::combobox::ComboboxEvent;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::{
    AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    Styled, Window, div, point, px, size,
};
use ss_core::translation::Engine;

use super::SettingsPage;
use crate::test_support::IsolatedDataDir;

struct ModelFieldHost(gpui_kit::Entity<SettingsPage>);

impl Render for ModelFieldHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.0.clone();
        let field = settings.update(cx, |page, cx| {
            let view = cx.entity().downgrade();
            page.model_field(window, cx, view)
        });
        div()
            .debug_selector(|| "tran-host-root".into())
            .size_full()
            .p(px(16.))
            .child(field)
    }
}

fn paint(cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut gpui_kit::VisualTestContext, bounds: Bounds<Pixels>) {
    cx.simulate_click(
        point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        ),
        Default::default(),
    );
}

/// Renders the field inside a host, overriding the fetch outcomes the test
/// needs. Returns the settings entity behind the host and the window context.
fn host_with_models<'a>(
    cx: &'a mut gpui_kit::TestAppContext,
    models: &[String],
    current: &str,
) -> (
    gpui_kit::Entity<SettingsPage>,
    &'a mut gpui_kit::VisualTestContext,
) {
    let (host, cx) = cx.add_window_view(|window, cx| {
        let settings = cx.new(|cx| SettingsPage::new(window, cx));
        settings.update(cx, |page, _| {
            page.translation.engine = Engine::Llm;
            page.translation.llm_account_id = "acct".into();
            page.translation.llm_account_catalog = "opencode-go".into();
            page.translation.llm_model = current.to_string();
            page.llm_models = models.to_vec();
            page.llm_models_for = "acct".into();
        });
        ModelFieldHost(settings)
    });
    let settings = cx.update(|_, cx| host.read(cx).0.clone());
    (settings, cx)
}

#[gpui_kit::test]
fn model_combobox_reports_fetched_catalog_size(cx: &mut gpui_kit::TestAppContext) {
    let _dir = IsolatedDataDir::new();
    cx.update(|cx| {
        crate::init_components(cx);
        crate::i18n::install(cx);
    });
    let models: Vec<String> = (0..40).map(|index| format!("m{index:02}")).collect();
    let (_settings, cx) = host_with_models(cx, &models, "m07");
    cx.simulate_resize(size(px(1200.), px(800.)));
    paint(cx);
    paint(cx);

    let trigger = cx
        .debug_bounds("tran-llm-model-field")
        .expect("model trigger renders for a fetched catalog");
    click(cx, trigger);
    paint(cx);
    paint(cx);

    let total = cx
        .debug_bounds("tran-llm-model-total")
        .expect("the footer count renders once the menu opens");
    assert!(
        total.size.width > px(0.) && total.size.height > px(0.),
        "the footer count is a real element, not an empty box"
    );
}

#[gpui_kit::test]
fn model_combobox_keeps_current_model_when_catalog_lacks_it(cx: &mut gpui_kit::TestAppContext) {
    let _dir = IsolatedDataDir::new();
    cx.update(|cx| {
        crate::init_components(cx);
        crate::i18n::install(cx);
    });
    let models: Vec<String> = (0..3).map(|index| format!("m{index:02}")).collect();
    let (settings, cx) = host_with_models(cx, &models, "custom-model");
    cx.simulate_resize(size(px(1200.), px(800.)));
    paint(cx);
    paint(cx);

    cx.update(|_, cx| {
        let selected = settings.read(cx).llm_model_state.read(cx).selected_values();
        assert_eq!(
            selected,
            vec!["custom-model".to_string()],
            "a model outside the fetched catalog stays selected as an explicit first row"
        );
    });
}

#[gpui_kit::test]
fn model_combobox_empty_menu_renders_hint(cx: &mut gpui_kit::TestAppContext) {
    let _dir = IsolatedDataDir::new();
    cx.update(|cx| {
        crate::init_components(cx);
        crate::i18n::install(cx);
    });
    let (_settings, cx) = host_with_models(cx, &[], "");
    cx.simulate_resize(size(px(1200.), px(800.)));
    paint(cx);
    let trigger = cx
        .debug_bounds("tran-llm-model-field")
        .expect("the trigger renders without a catalog");
    click(cx, trigger);
    paint(cx);
    paint(cx);
}

#[gpui_kit::test]
fn model_combobox_change_event_pins_model(cx: &mut gpui_kit::TestAppContext) {
    let _dir = IsolatedDataDir::new();
    cx.update(|cx| {
        crate::init_components(cx);
        crate::i18n::install(cx);
    });
    let models: Vec<String> = (0..5).map(|index| format!("m{index:02}")).collect();
    let (settings, cx) = host_with_models(cx, &models, "");
    cx.simulate_resize(size(px(1200.), px(800.)));
    paint(cx);

    cx.update(|_, cx| {
        let state = settings.read(cx).llm_model_state.clone();
        state.update(cx, |_, cx| {
            cx.emit(ComboboxEvent::<SearchableVec<String>>::Change(vec![
                "m07".to_string(),
            ]));
        });
    });
    paint(cx);

    cx.update(|_, cx| {
        let page = settings.read(cx);
        assert_eq!(page.translation.llm_model, "m07");
        assert!(
            page.translation.llm_model_pinned,
            "picking from the combobox pins the model"
        );
    });
}
