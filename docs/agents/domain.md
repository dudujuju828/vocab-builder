# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — the Vocab glossary (Word, Sighting, Definition, Note, Book, Library, Current Book).
- **`docs/adr/`** — read ADRs that touch the area you're about to work in.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront. The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily when terms or decisions actually get resolved.

## File structure

This is a **single-context** repo:

```
/
├── CONTEXT.md
├── docs/
│   ├── adr/
│   │   ├── 0001-bundled-offline-dictionary.md
│   │   ├── 0002-rust-and-ratatui.md
│   │   ├── 0003-full-screen-screens-not-a-repl.md
│   │   └── 0004-deepseek-for-ai-notes.md
│   └── specs/
└── src/
```

If this ever grows into multiple bounded contexts, the layout becomes a root `CONTEXT-MAP.md` pointing at one `CONTEXT.md` per context, with context-scoped decisions under `src/<context>/docs/adr/` alongside system-wide ones in `docs/adr/`.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids — a `Sighting` is never an "entry" or an "occurrence"; a `Note` is never an "AI definition".

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0001 (bundled offline dictionary) — but worth reopening because…_
