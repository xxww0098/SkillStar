# skillstar-security-scan: learnings

## 1. Workspace extraction pattern observed

Existing crates (`skillstar-infra`, `skillstar-ai`, `skillstar-skills`, `skillstar-model-config`) follow a clear pattern:
- Each is a standalone `[package]` in `crates/`
- `src-tauri/Cargo.toml` declares them as versioned path dependencies
- They export through `lib.rs` and are re-exported at the app's `core/mod.rs`
- App crate (`src-tauri`) stays as the thin Tauri command wiring layer

## 2. What to extract — boundaries found

The `security_scan` module is surprisingly clean for extraction:
- **Pure domain logic**: types, orchestrator, static analyzers, policy, smart rules — all in `core/security_scan/`
- **No Tauri-specific imports** in most files (only `mod.rs` uses `crate::core::infra::paths` and `crate::core::ai_provider`)
- Thin app-specific wiring: `security_scan/mod.rs` line 20 uses `super::ai_provider` — this is the **pivot point**
- Report generation (SARIF/JSON/MD/HTML) is pure formatting — ideal for the crate itself

**Avoid over-extracting**: Do NOT move `commands/` security-scan command handlers. Keep those as thin invoke wrappers in the app crate that delegate to the extracted crate.

## 3. Key dependency challenge: ai_provider

`ai_provider` is currently in `src-tauri/src/core/ai_provider/`. Security scan's orchestrator uses `chat_completion` from it.

**Two options**:
- **Option A (preferred)**: `skillstar-scan` depends on `skillstar-ai`. The `skillstar-ai` crate already re-exports from `ai_provider`. Add `chat_completion` to its public API surface if not already exposed.
- **Option B**: Keep a thin `AiConfig` + `chat_completion` shim in the app crate that forwards to `skillstar-ai`.

Option A is cleaner. Verify `skillstar-ai` already exposes `AiConfig` and `chat_completion`.

## 4. Path / infra dependency

`security_scan/mod.rs` uses `crate::core::infra::paths::security_scan_log_path()` and `crate::core::infra::paths::security_scan_policy_path()`.

**Solution**: The extracted crate should own its own paths module (e.g., `skillstar-scan/src/paths.rs`) that computes paths relative to `dirs::data_dir()` or a configurable root. Do NOT make the scan crate depend on `skillstar-infra` directly — that creates a circular dependency risk. Instead, accept a data root path as a config parameter.

## 5. SQLite cache

Security scan uses `rusqlite::Connection` (from `skillstar-infra`'s db pool) for caching. The `ClearSecurityScanCache` Tauri command uses this.

**Solution**: The scan crate should own its own SQLite connection string/path, opened lazily. It should NOT depend on `skillstar-infra::db_pool`. The crate can depend on `rusqlite` directly (as `skillstar-infra` does) for its own caching.

## 6. Prompt loading

Security scan loads prompts at compile time from `src-tauri/prompts/security/`. These should move to the scan crate as embedded resources or include_str! data.

## 7. Verification checklist

See `issues.md` for problems found during research.

---

## 8. T34 Capability Risk Detector — Capability-Detection Surface Map

### 1. Current Capability-Detection Code Paths

#### Primary: `SkillDocConsistencyAnalyzer` (orchestrator.rs lines 208–473)

The main capability-risk detector today. Struct is `SkillDocConsistencyAnalyzer` at line 208.

**Key struct: `SkillCapabilityRule`** (lines 211–219):
```rust
struct SkillCapabilityRule {
    id_suffix: &'static str,           // e.g. "network", "command_exec"
    capability_label: &'static str,     // e.g. "network access"
    code_needles: &'static [&'static str],   // signals IN code
    doc_needles: &'static [&'static str],    // what SKILL.md should say
    restrictive_doc_needles: &'static [&'static str], // "does NOT..." phrases
    undeclared_severity: RiskLevel,
    contradiction_severity: RiskLevel,
}
```

**5 capability rules defined** (`SKILL_DOC_CAPABILITY_RULES`, lines 221–369):

| Rule ID | Label | Undeclared Severity | Contradiction Severity |
|---------|-------|---------------------|------------------------|
| `network` | network access | Medium | High |
| `command_exec` | command execution | High | Critical |
| `file_write` | file modification | Medium | High |
| `secret_access` | environment or secret access | High | Critical |
| `persistence` | persistence or startup modification | High | Critical |

**Generated finding pattern IDs**:
- `skill_doc_contradiction_{id_suffix}` — SKILL.md prohibits but code does
- `skill_doc_undeclared_{id_suffix}` — code has capability but SKILL.md doesn't disclose

**Critical gap**: `taxonomy: None` on ALL findings — lines 441 and 462. The taxonomy field exists but is never populated.

#### Secondary: `SemanticFlowAnalyzer` (orchestrator.rs)

Call-graph taint analysis. Does NOT produce explicit "capability" findings. ID: `"semantic"`. Uses `graph_from_files()` and `find_paths()` in `static_patterns.rs`.

#### Tertiary: `DynamicSandboxAnalyzer` (orchestrator.rs)

Detects sandbox capability gaps. Produces findings:
- `dynamic_sandbox_partial` — partial sandbox (Medium)
- `dynamic_sandbox_degraded` — missing/degraded sandbox (High)

### 2. Taxonomy Integration Points (T33 → T34)

**All in `crates/skillstar-security-scan/src/types.rs`:**

| Type | Lines | Purpose |
|------|-------|---------|
| `DetectionFamily::Capability` | 14–32 | Enum variant for capability family |
| `DetectionKind::capability(kind)` | 84–86 | Shorthand constructor |
| `DetectionTaxonomy::with_kind()` | 130–135 | Helper to create taxonomy with kind |
| `StaticFinding.taxonomy: Option<DetectionTaxonomy>` | ~323 | Finding field (currently always None) |
| `AiFinding.taxonomy: Option<DetectionTaxonomy>` | ~383 | AI finding field (currently always None) |

**The wiring gap**: Both `SkillDocConsistencyAnalyzer::scan()` (line 441) and AI analysis (`scan.rs` line 2205, 2468) set `taxonomy: None`. Taxonomy infrastructure is complete but unused.

**Smart rules integration** (`smart_rules.rs` + `security_smart_rules_default.yaml`):
- `network_signals` rule — detects network URLs in resource files
- `command_execution_signals` rule
- `persistence_signals` rule
- `env_access_signals` rule
- These could feed a T34 capability risk aggregator

### 3. Existing Tests / Behavior Contract

#### Direct capability/doc-consistency tests:

| Test | File | Line | What it covers |
|------|------|------|----------------|
| `doc_consistency_analyzer_flags_skill_doc_contradictions` | scan.rs | 4573 | SKILL.md "read-only/offline" vs `curl \| sh` → `skill_doc_contradiction_` finding |
| `detection_kind_shorthand_constructors` | types.rs | ~590 | `DetectionKind::capability()` produces correct family=Capability |
| `ai_finding_roundtrip_with_taxonomy` | types.rs | ~680 | AiFinding with capability taxonomy roundtrips through serde |
| `dynamic_behavior_marks_partial_sandbox` | orchestrator.rs | 2639 | DynamicSandboxAnalyzer emits `dynamic_sandbox_partial` |
| `dynamic_behavior_marks_missing_sandbox_as_high` | orchestrator.rs | 2663 | DynamicSandboxAnalyzer emits `dynamic_sandbox_degraded` at RiskLevel::High |

#### Indirect coverage via policy preset tests:

| Test | File | Line | What it covers |
|------|------|------|----------------|
| `enabled_analyzers_follow_preset_when_not_configured` | scan.rs | ~4700 | balanced preset includes doc_consistency; strict includes all |

**No dedicated T34-specific tests exist** — capability detection is exercised only through the single `doc_consistency_analyzer_flags_skill_doc_contradictions` test and the `DetectionKind::capability()` constructor roundtrip.

### 4. Recommended Minimal File Scope for T34

**T34 scope: Wire taxonomy into existing `SkillDocConsistencyAnalyzer` findings.**

Minimal change set (no new files, no new tests strictly required):

```
orchestrator.rs:
  Line 441: taxonomy: None  →  taxonomy: Some(DetectionTaxonomy::with_kind(DetectionKind::capability(rule.id_suffix)))
  Line 462: taxonomy: None  →  taxonomy: Some(DetectionTaxonomy::with_kind(DetectionKind::capability(rule.id_suffix)))

types.rs / scan.rs:
  AI analysis path also sets taxonomy: None — wire AI findings to taxonomy similarly when AI emits capability-category findings
```

**What T34 must NOT expand into (T35 scope)**:
- New analyzer structs in orchestrator.rs
- New `SkillCapabilityRule` definitions beyond the 5 existing rules
- Changes to `SemanticFlowAnalyzer` or `DynamicSandboxAnalyzer`
- Modifications to the smart_rules engine
- Changes to report generation (SARIF/JSON/MD/HTML)
- Modifications to the policy loading/resolution system

**If T34 adds a dedicated capability risk aggregator** (not just wiring existing findings):
- New file: `src/capability_risk.rs` in the scan crate
- New struct: `CapabilityRiskAggregator` that consumes `StaticFinding` / `AiFinding` where `taxonomy.detection_kind.family == DetectionFamily::Capability`
- Produces a `CapabilityRiskReport` summarizing risk by category
- This is clean extension that doesn't touch existing analyzers
