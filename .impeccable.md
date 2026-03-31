# SkillStar — Impeccable Design Context

## Users
Power developers orchestrating multiple agent CLIs across projects. They value precision, low noise, and predictable tooling.

## Brand
**Precise. Unified. Effortless.**

- Voice: technical, concise, no fluff.
- Emotional goal: confidence and control.
- Tagline: `less is more`.

## Visual Direction
Dark glassmorphism is primary; Paper light mode is optional.

- Primary background: `#0a0a0f`
- Light background: `#f5f7fb`
- Style references: Linear, Vercel, Raycast
- Avoid: gamification, neon overload, cluttered dashboards

## Design Principles
1. Precision over decoration.
2. Dark-first, accessible always (AA contrast, focus visibility, reduced motion support).
3. Information density with clear spacing hierarchy.
4. Immediate feedback for success/error/loading states.
5. Chinese and English layouts must both remain stable.

## Tokens (Reference)
| Token | Dark | Paper |
|---|---|---|
| `background` | `#0a0a0f` | `#f5f7fb` |
| `foreground` | `#f4f4f5` | `#0f172a` |
| `primary` | `#3b82f6` | `#3b82f6` |
| `card` | `rgba(255,255,255,0.05)` | `#ffffff` |
| `border` | `rgba(255,255,255,0.1)` | `rgba(15,23,42,0.12)` |
| `muted-foreground` | `#a1a1aa` | `#64748b` |
| `success` | `#22c55e` | `#22c55e` |
| `warning` | `#f59e0b` | `#f59e0b` |
| `destructive` | `#ef4444` | `#ef4444` |

Typography: DM Sans (body), JetBrains Mono (code/metrics)

Radius: `6 / 8 / 12 / 16 / 24px`

Motion: Framer Motion spring easing `[0.22, 1, 0.36, 1]`, ~200ms transitions
