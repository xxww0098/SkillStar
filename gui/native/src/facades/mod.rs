//! Domain facades, one file per SkillStar domain, mirroring the ss-gpui
//! dependency set. Marketplace is the wave-1 bridge proof; the five
//! skill-cards slice facades are pre-registered empty so parallel slice
//! workers only ever add methods inside their own file here — never the
//! registry in `lib.rs`. Wave 2 (market base) added the marketplace read
//! side plus three pre-registered siblings: the detail-drawer and publisher
//! slices' empty facades and the translation read side. The remaining
//! domains (skills, accounts, settings, sync, storage) land with their
//! waves.

pub mod marketplace;
pub mod marketplace_drawer;
pub mod marketplace_publisher;
// Wave 5 (accounts): the subscriptions workbench facade (quota cards,
// activation handoff, reconcile, the three add-account flows). The
// reset-bank slice's redemption seam rides this file too (see its header).
pub mod accounts;
// Wave 4 (async bridge foundation) registration: the task/cancel/progress
// primitives module (bridge_task) plus the two slots of its parallel
// slices that remain (import modal, GitHub sign-in), mirroring how waves
// (D-111): the page's remote scope never had a real domain.
pub mod bridge_task;
pub mod my_skills_github;
pub mod my_skills_import;
// Wave 3 (my-skills base) registration: the page base's own facade plus
// three pre-registered empty slots for its parallel slices (detail drawer,
// selection bar, SKILL.md reader), mirroring how waves 1/2 pre-registered
// the deck and marketplace slices.
pub mod my_skills;
pub mod my_skills_drawer;
pub mod my_skills_reader;
pub mod my_skills_selection;
pub mod skill_cards;
pub mod skill_cards_create_group;
pub mod skill_cards_drawer;
pub mod skill_cards_import;
pub mod skill_cards_share;
pub mod translation;
// Projects page (projects/ mod+detail+agent_item+actions+upkeep+dialogs):
// the registered-project master-detail seam.
pub mod projects;
// Settings base wave: the page's own seam (gui_prefs.json + proxy.json,
// the base sections the base worker owns) plus two pre-registered empty
// slots for its parallel section slices (skill-updates/translation and
// gh-mirror/network-doctor), mirroring how the earlier waves pre-registered
// theirs. The slices only ever add methods inside their own file here.
pub mod settings;
pub mod settings_network;
pub mod settings_updates;
// Wave 7 (settings sections 2): two more pre-registered empty slots, one
// for the agent-connections/managed-skills slice, one for the
// storage/skill-repair/about slice — same rule as the earlier waves: the
// slices only ever add methods inside their own file here.
pub mod settings_agent_connections;
pub mod settings_storage;

// 所有 facade 测试借用此锁串行化 SKILLSTAR_DATA_DIR 沙箱：每个切片的
// IsolatedDataDir 都 set_var/remove_var 这一个进程级环境变量，cargo 并行
// 线程下模块内各自的锁互不互斥，交叉时读到真实 ~/.skillstar 而panic
// （该二进制 panic 直接 abort）。borrow it for the whole sandbox lifetime:
// `let _guard = super::TEST_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());`
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
