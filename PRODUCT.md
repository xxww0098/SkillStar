# Product

<!-- impeccable:product-schema 1 -->

## Platform

web — React SPA embedded in a Tauri v2 desktop shell (WKWebView on macOS). The design language is desktop-app dense, not marketing-page sparse; it must read correctly in both bundled themes (dark "OLED" default and light "paper").

## Stack

Tauri v2 + React + TypeScript + Tailwind CSS v4 (theme tokens in src/index.css) + Radix primitives + react-i18next (zh-CN / en). Frontend reaches the Rust workspace only through Tauri invoke() and events.

## Users

Developers who run several CLI/desktop coding agents (Claude Code, Codex, OpenCode, Cursor…) across multiple login accounts and subscriptions, and want one place to install and distribute skills, switch the account a CLI is actually serving, and watch usage — without configuring or wiring models (the model domain was removed wholesale, D-082).

## Product Purpose

SkillStar is the control surface around the user's agent fleet for three jobs: install/distribute skills, manage and switch multiple login accounts per provider, and display usage (quota windows, today's sessions, estimated cost). Success = an agent is serving the account the user picked, its skills are where they should be, and the usage page explains what was spent.

## Positioning

Neighboring tools manage one tool's accounts or one skill directory; SkillStar brokers the whole set — per-provider multi-account switching against the tools' real credential stores, plus a local-first skill marketplace and deployment. It deliberately does not route, configure, or serve models anymore.

## Operating Context

Desktop app living next to the user's terminals and agents; Chinese-first UI with English fallback (mixed-locale copy is a known defect, not a choice). Dense technical data (quota windows, token counts, paths) is the norm; JetBrains Mono is reserved for code/ids/numbers, DM Sans for prose.

## Capabilities and Constraints

- Surfaces: My Skills, Marketplace, Skill Cards, Projects, Settings (skills mode); Accounts (multi-account management: add/switch/import); Usage (read-only consumption view); plus lightweight usage card windows.
- Security: never render raw credentials; keys live in local AES-256-GCM JSON, never the system keychain (D-072). Claude account switching is file-based off macOS and unavailable on macOS by that same policy.
- Accessibility: existing keyboard shortcuts, command palette, focus rings, and reduced-motion handling must survive any visual change.

## Product Principles

- Local and legible: the UI explains which account a CLI is serving (disk truth, not a cached pin) without exposing secrets.
- One concept per surface: skills (install/distribute), accounts (who is logged in), usage (what was spent) — no card soup.
- Copy is part of the product: controls name their action in the user's locale.
- The page must earn its density: technical data stays compact; chrome stays quiet.
