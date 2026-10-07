# Domain Docs

SkillStar 的词汇表是根目录 `CONTEXT.md`。长期架构决策在 `docs/decisions.md`。本仓库没有 `docs/adr/`。

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root.
- **`docs/decisions.md`** for architecture decisions that touch the area you are about to change.

If a file a generic skill mentions does not exist here, proceed silently. Don't suggest creating `docs/adr/` or a second glossary.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag decision conflicts

If your output contradicts an existing entry in `docs/decisions.md`, surface it explicitly rather than silently overriding:

> _Contradicts D-0NN (title) — but worth reopening because…_
