# Ultranano Cleanup Design

**Goal:** Remove leftover dead code and warnings from the pre-plugin snapshot while preserving current editor behavior and keeping the project lightweight.

## Scope

This cleanup explicitly does not add features, change keybindings, alter file formats, or introduce new dependencies. The repository is already free of plugin code, so the remaining work is a targeted maintenance pass on the current core editor.

## Planned Changes

### 1. Remove unused code and imports

- Delete unused imports in `src/editor.rs` and `src/main.rs`.
- Remove editor helper methods that are not referenced anywhere in the current pre-plugin codebase.
- Remove dead helper functions duplicated from older iterations where they no longer contribute to runtime behavior.

### 2. Simplify equivalent control flow

- Tighten save and prompt-handling branches in `src/main.rs` where the current logic can be expressed more directly without changing outcomes.
- Keep prompt behavior, exit behavior, and save semantics exactly the same.

### 3. Keep rendering logic minimal and readable

- Make small readability cleanups in `src/render.rs` only where the rendered output and cursor behavior remain unchanged.
- Avoid adding abstractions or indirection that would make the editor heavier.

## Non-Goals

- No plugin system changes, because the clean repo snapshot already excludes plugin support.
- No UI redesign or new commands.
- No dependency additions.
- No architecture rewrite.

## Verification

- Run `cargo check`.
- Run `cargo test`.
- Confirm no plugin-related symbols or files were introduced.
- Review warnings after cleanup and aim to remove the current obvious dead-code and unused-import warnings without changing runtime behavior.

## Risks

The main risk is accidental behavior drift in prompt submission, save/exit flow, or rendering. To avoid that, cleanup will stay local, avoid semantic rewrites, and preserve existing public behavior.
