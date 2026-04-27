# skillstar-ai Extraction Issues

## Pre-existing blockers (not caused by T11)

- Full test suite (`cargo test`) has unrelated failures in other modules  
  → Verification must proceed at package-level, not full-suite level
- No `machine_uid::get()` in extracted crate context yet  
  → Need to design path injection before extracting `mod.rs` config load/save

## Decisions pending

1. Should `skillstar-ai` crate depend on `skillstar-config` for path config,
   or receive paths as runtime parameters?
2. Does `AiConfig` include translation settings (`translation_api`, `translation_settings`)?
   → Currently yes (in `config.rs`), but these may move to `skillstar-translation` later
3. Should `translation_looks_translated` move to `skillstar-translation` or stay in
   `skillstar-ai` as a generic validation helper?
