//! P0 first-party Guide: version-bound frontend-design seed.
//!
//! Marketplace record at land time is anthropics/skills (not a name-only
//! association). The pinned SkillRevision is the catalog commit/tree plus a
//! documented seed content hash. Live hub snapshots are compared by the app
//! overlay; they never rewrite this document.

use super::{
    CalloutTone, Guide, GuideBlock, GuideId, GuideStep, GuideStepKind, blocks::BLOCK_SCHEMA_VERSION,
};
use crate::identity::{
    ContentRevision, GitTrackingRef, SkillIdentity, SkillRevision, SkillRevisionKey,
};

pub const SEED_GUIDE_ID: &str = "guide:frontend-design-first-success";
pub const SEED_LOCALE: &str = "zh-CN";
pub const SEED_REPOSITORY: &str = "https://github.com/anthropics/skills";
pub const SEED_CONTENT_ROOT: &str = "skills/frontend-design";
pub const SEED_COMMIT: &str = "53048666b05b4799081517d00e09e0a2dd688678";
pub const SEED_TREE: &str = "0d5b74a14bdf3ebcd64f352d06376a2ef05ed296";
/// Documented seed pin (`sha256(skillstar.seed.frontend-design.p0.v1)`), not a
/// live v2 snapshot of the cloned tree. Installed content that differs is a
/// skill-drift signal, not a silent progress rewrite.
pub const SEED_CONTENT_HASH: &str =
    "sha256:08c665c6594b90d2bc781094e7afd6a9cd9296fde61e6ff8e7b53e61b9b1fe1f";
pub const SEED_DISPLAY_NAME: &str = "frontend-design";

pub fn seed_identity() -> SkillIdentity {
    SkillIdentity::git(
        SEED_REPOSITORY,
        GitTrackingRef::DefaultBranch,
        SEED_CONTENT_ROOT,
    )
    .expect("seed identity is well-formed")
}

pub fn seed_revision() -> SkillRevision {
    let identity = seed_identity();
    SkillRevision::git(
        &identity,
        SEED_COMMIT,
        SEED_TREE,
        ContentRevision::new(2, SEED_CONTENT_HASH).expect("seed content hash is well-formed"),
    )
    .expect("seed revision is well-formed")
}

#[allow(dead_code)]
pub fn seed_skill_revision_key() -> SkillRevisionKey {
    seed_revision().key
}

pub fn frontend_design_first_success() -> Guide {
    let identity = seed_identity();
    let revision = seed_revision();
    Guide::new(
        GuideId::new(SEED_GUIDE_ID).expect("seed guide id"),
        "第一次用 frontend-design 做出可用界面",
        SEED_LOCALE,
        "用 frontend-design 走完一次界面实践。阅读不需要安装；只有动手改文件时才安装精确 revision。",
        BLOCK_SCHEMA_VERSION,
        identity,
        revision,
        vec![
            GuideStep {
                id: "s1-when".into(),
                kind: GuideStepKind::Reading,
                title: "适用场景与边界".into(),
                requires_skill: false,
                blocks: vec![
                    GuideBlock::Paragraph {
                        text: "frontend-design 帮你把一个界面想法做成可运行的前端，并避免落到「任何产品都长这样」的模板默认。".into(),
                    },
                    GuideBlock::List {
                        ordered: false,
                        items: vec![
                            "适合：新页面、重塑现有界面、需要明确视觉立场的 brief。".into(),
                            "不适合：只改文案、只接 API、或把安装本身当成目标。".into(),
                            "阅读这份 Guide 不要求本地已安装 Skill。".into(),
                        ],
                    },
                    GuideBlock::Callout {
                        tone: CalloutTone::Info,
                        text: "身份来自 marketplace 记录 anthropics/skills + skills/frontend-design，不是 skill 名字。".into(),
                    },
                ],
            },
            GuideStep {
                id: "s2-how".into(),
                kind: GuideStepKind::Reading,
                title: "怎么做".into(),
                requires_skill: false,
                blocks: vec![
                    GuideBlock::Paragraph {
                        text: "先把 brief 钉死，再让 Agent 按 frontend-design 生成，只接受可运行的结果。".into(),
                    },
                    GuideBlock::List {
                        ordered: true,
                        items: vec![
                            "描述目标界面、受众和这一页要完成的一件事。".into(),
                            "要求 Agent 先给色彩/字体/布局/签名元素的短计划，再写代码。".into(),
                            "拒绝奶油衬线+陶土、近黑+酸绿、或无主题的报纸分栏这三种默认。".into(),
                            "只接受能在浏览器里跑起来的界面，不要停在口头描述。".into(),
                        ],
                    },
                    GuideBlock::Callout {
                        tone: CalloutTone::Warning,
                        text: "这一步仍是阅读。勾选完成不会安装 Skill，也不会执行作者命令。".into(),
                    },
                ],
            },
            GuideStep {
                id: "s3-practice".into(),
                kind: GuideStepKind::Practice,
                title: "在真实项目里改一处界面".into(),
                requires_skill: true,
                blocks: vec![
                    GuideBlock::Paragraph {
                        text: "动手需要精确 revision 的本地文件。未安装时会预览身份与权限，确认后才安装；P0 不自动执行作者命令。".into(),
                    },
                    GuideBlock::List {
                        ordered: true,
                        items: vec![
                            "打开一个真实项目，选一处需要视觉立场的界面。".into(),
                            "把 brief 和 frontend-design 一起交给已启用的 Agent。".into(),
                            "检查结果是否可运行，以及是否避开了模板默认。".into(),
                        ],
                    },
                    GuideBlock::Callout {
                        tone: CalloutTone::Warning,
                        text: "安装按钮是显式动作。预览里看到的 commit/tree/content hash 必须与本 Guide 绑定的 revision 一致。".into(),
                    },
                ],
            },
            GuideStep {
                id: "s4-verify".into(),
                kind: GuideStepKind::Verify,
                title: "对照验收清单".into(),
                requires_skill: false,
                blocks: vec![
                    GuideBlock::Paragraph {
                        text: "用下面的清单判断这次实践是否成功。验证步骤不触发安装。".into(),
                    },
                    GuideBlock::List {
                        ordered: false,
                        items: vec![
                            "页面能运行，不是静态 mock 描述。".into(),
                            "色彩、字体和签名元素能说出与 brief 的对应关系。".into(),
                            "没有把「安装 Skill」当成完成条件。".into(),
                            "尊重减少动效，键盘焦点可见。".into(),
                        ],
                    },
                ],
            },
        ],
    )
    .expect("seed guide is well-formed")
}
