# Product

<!-- impeccable:product-schema 1 -->

## Platform

web — React SPA embedded in a Tauri v2 desktop shell (WKWebView on macOS). The design language is desktop-app dense, not marketing-page sparse; it must read correctly in both bundled themes (dark "OLED" default and light "paper").

## Stack

Tauri v2 + React + TypeScript + Tailwind CSS v4 (theme tokens in src/index.css) + Radix primitives + react-i18next (zh-CN / en). Frontend reaches the Rust workspace only through Tauri invoke() and events.

## Users

Inferred (interview unanswered — ask_user_question timed out): developers who run several CLI/desktop coding agents (Claude Code, Codex, OpenCode, Pi…) and want one place to wire each agent to model providers through a local gateway, watch usage/cost, and manage skills.

## Product Purpose

SkillStar is the control surface around the user's agent fleet: bind each agent to a provider/model via the local gateway, group and route providers, track spend, and install/sync skills. Success = an agent is connected and calling through the gateway with the routing the user chose, observable in the recent-calls ledger.

## Positioning

One local loopback gateway (127.0.0.1) fronts every provider: agents only ever see local endpoints, while provider credentials, routing policy, and profile presets live in SkillStar. Neighboring tools configure one agent or one provider; this one brokers the whole set.

## Operating Context

Desktop app living next to the user's terminals and agents; Chinese-first UI with English fallback (mixed-locale copy is a known defect, not a choice). Dense technical data (endpoints, model refs, status codes, latencies) is the norm; JetBrains Mono is reserved for code/ids/numbers, DM Sans for prose.

## Capabilities and Constraints

- Surfaces: My Skills, Marketplace, Skill Cards, Projects, Settings (skills mode); Models hub (agents/providers/gateway); Usage (subscription tracker); plus lightweight usage card windows.
- Security: never render raw credentials or vendor URLs — masked summaries and loopback addresses only (enforced by tests).
- Accessibility: existing keyboard shortcuts, command palette, focus rings, and reduced-motion handling must survive any visual change.
- Undecided (inferred, not confirmed): full-app redesign vs. the Models page first — the Models page is the confirmed pain point.

## Product Principles

- Local and legible: agents talk to 127.0.0.1; the UI explains where traffic goes without exposing secrets.
- One concept per surface: pick (rail), configure (gateway), observe (calls) — no card soup.
- Copy is part of the product: controls name their action in the user's locale.
- The page must earn its density: technical data stays compact; chrome stays quiet.
