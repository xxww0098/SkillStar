fn main() -> anyhow::Result<()> {
    ss_gpui::run(env!("CARGO_PKG_VERSION"))
}
