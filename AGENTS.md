# GitSpark UI Agent Guide

GitSpark is a native Rust desktop app built on `eframe/egui`.

This repo should not be treated like a web app:
- no DOM
- no CSS cascade
- no browser layout engine
- no retained widget tree

`egui` is immediate mode. Precision comes from explicit sizing, stable IDs, custom painting discipline, and reusable primitives.

## Core Rule

Do not add more ad hoc UI to `src/ui/app.rs` when a reusable primitive or component should exist.

If a pattern appears twice, it is a candidate for extraction.

## Current Repo Reality

The codebase already needs a component system:
- `src/ui/app.rs` is still the orchestration layer and too much of the UI layer
- `src/ui/theme.rs` has tokens, but not enough structural guidance
- `src/ui/components/diff.rs` is already a specialized renderer
- repeated rows, dropdowns, inputs, and popup behavior still drift across screens

The right direction is a small project-specific UI kit on top of `egui`, not a giant abstraction layer.

## Required Architecture

Use this shape:

- `src/ui/theme.rs`
  - colors
  - geometry tokens
  - spacing
  - radii
  - shared frame helpers
- `src/ui/primitives/`
  - `button.rs`
  - `text_input.rs`
  - `dropdown.rs`
  - `surface.rs`
  - `row.rs`
  - `badge.rs`
- `src/ui/components/`
  - screen-level components built from primitives
- `src/ui/components/diff.rs`
  - stays specialized

Screens should compose primitives and return actions. They should not own async orchestration.

## Egui Caveats Agents Must Respect

- Widgets are builders, not state holders.
- UI state lives in `egui::Memory`.
- Repeated stateful widgets need stable IDs.
- Child `Ui`s can share IDs unless salted.
- Low-level child layouts do not reserve parent space automatically.
- Popups must be kept alive every frame while open.
- Scroll state is keyed by ID.
- Manual painting requires explicit rect allocation and clipping discipline.

Primary references:
- https://github.com/emilk/egui
- https://docs.rs/egui/latest/egui/struct.Ui.html
- https://docs.rs/egui/latest/egui/struct.UiBuilder.html
- https://docs.rs/egui/latest/egui/struct.Id.html
- https://docs.rs/egui/latest/egui/struct.Memory.html
- https://docs.rs/egui/latest/egui/struct.Painter.html
- https://docs.rs/egui/latest/egui/containers/scroll_area/struct.ScrollArea.html
- https://docs.rs/egui/latest/egui/containers/struct.Popup.html

## Stable ID Rule

Every repeated row, popup, selector, checkbox-like control, text input, and scroll area must have an explicit stable ID strategy.

Preferred ID sources:
- repo path
- file path
- commit SHA
- branch name
- enum key

Use:
- `ui.push_id(...)`
- `UiBuilder::id_salt(...)`
- `Id::with(...)`

Do not use unstable auto-generated IDs for repeated interactive widgets.

## Primitive Responsibilities

### `button.rs`
- primary button
- secondary button
- ghost button
- icon button
- tab button

### `text_input.rs`
- dark single-line field
- dark password field
- dark multiline field
- fixed-height scrolling multiline variant when needed

### `dropdown.rs`
- popup lifecycle
- popup ID
- dark popup frame
- close-on-select behavior
- menu row styling
- width rules

### `surface.rs`
- panel frame
- surface frame
- card frame
- bordered section

### `row.rs`
- selectable row
- file row
- commit row
- settings nav row

## Precision Rules

- Put repeated heights in theme tokens.
- Put repeated paddings in theme tokens.
- Put repeated radii in theme tokens.
- Avoid copy-pasting `Frame::default()` chains.
- Avoid hand-tuning the same offsets in multiple places.

When painting manually:
- allocate the rect first
- separate hit rect from paint rect
- keep hover, selected, and disabled states explicit
- return `egui::Response` for interactive controls

## Popup Rules

All dropdowns and menus should use one shared popup primitive.

Do not hand-roll popup styling separately for:
- toolbar menus
- settings selectors
- filter menus
- context-like menus

## Scroll Rules

For large repeated content:
- prefer `ScrollArea::show_rows(...)` or viewport-aware rendering
- give the scroll area a stable ID
- define row height explicitly

Apply this especially to:
- changes list
- history list
- repo selector
- model selector
- commit file list

## Composition Rules

`src/ui/app.rs` should become:
- app state
- event loop
- action dispatch
- top-level screen composition

It should not keep:
- custom dropdown implementations
- custom text field styling
- repeated row rendering logic
- repeated popup lifecycle code

## Repo-Specific Do / Don't

Do:
- reuse `src/ui/theme.rs`
- move repeated UI logic out of `src/ui/app.rs`
- keep `src/ui/components/diff.rs` specialized
- prefer small Rust functions over giant render closures

Do not:
- add more stub modules without moving real logic into them
- add more duplicated geometry constants inline
- rely on default `egui` visuals for production UI
- leave repeated interactive widgets without explicit IDs

## Definition Of Done For UI Work

A UI change is not done unless:
- repeated patterns are extracted when appropriate
- IDs are stable
- hover/active/selected states are explicit
- geometry comes from tokens or helpers
- scroll behavior is deliberate
- `src/ui/app.rs` is simpler, not more crowded

## Immediate Refactor Priority

1. Expand `src/ui/theme.rs`
2. Standardize dropdowns and rows
3. Standardize inputs
4. Extract sidebar, toolbar, settings, and commit patterns
5. Keep `diff.rs` specialized, but aligned to shared tokens

That is the baseline for making GitSpark precise and production-safe on top of `egui`.
