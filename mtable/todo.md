# Cardmat Improvement Roadmap

## High Priority: Architecture & Simplification
- [x] **Unify "Draggable" Entities**: Create a `BoardObject` enum or trait to unify `Card` and `Counter`. This will eliminate duplicated logic for selection, dragging, and Z-order management.
- [ ] **Decompose the `update` Function**: Split the massive `update` loop into three distinct phases: `handle_input()`, `update_state()`, and `draw()`.
- [ ] **Extract Rendering Logic**: Move the drawing logic (artwork grids, text boxes, rotation) into `impl Card` and `impl Counter` (or the unified `BoardObject`).
- [ ] **Simplify Z-Order Management**: Use a single unified list of objects so that "bringing to front" is a simple `swap_remove` and `push` operation.

## Medium Priority: Logic & Refinement
- [ ] **Refactor "Pending Creation" Logic**: Implement a more declarative state machine for the "drag-out" creation process to separate button hovering from world placement.
- [ ] **Streamline Selection Logic**: Simplify right-click selection by using a unified object list and a single `.filter()` call for `Rect` intersections.
- [ ] **Move Artwork Generation to `Card`**: Move the pixel-jittering logic from `Deck` to `Card` or a dedicated `Artwork` struct.
- [ ] **Use a Unified ID System**: Wrap `u64` in an `EntityId` type to prevent accidental ID mixing and improve type signatures.

## Low Priority: Polish & Maintenance
- [ ] **Consolidate Magic Numbers**: Move hardcoded visual constants (e.g., artwork height ratios, padding) to the top of `app.rs`.
- [ ] **Improve Coordinate Transformations**: Create helper methods for converting between `WorldPos` and `ScreenPos` to reduce repetitive `(pos * zoom) + offset` math.

## Implementation Phases

### Phase 1: Foundation & Type Safety
- [ ] Use a Unified ID System
- [ ] Improve Coordinate Transformations
- [ ] Consolidate Magic Numbers

### Phase 2: Structural Unification
- [x] Unify "Draggable" Entities & Simplify Z-Order
- [ ] Streamline Selection Logic

### Phase 3: Logic Decomposition
- [ ] Extract Rendering Logic
- [ ] Decompose the `update` Function
- [ ] Move Artwork Generation to `Card`

### Phase 4: Feature Refinement
- [ ] Refactor "Pending Creation" Logic
