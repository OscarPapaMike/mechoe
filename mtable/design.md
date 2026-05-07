# Cardmat Design Document

## Overview
Cardmat is a Rust-based application built with `eframe` and `egui` that simulates a digital playmat for card games. It allows users to spawn cards, manipulate them on a 2D plane, organize them using a snapping grid, and manage groups of cards through selection tools.

## Core Features

### 1. Card Management
- **Deck Drawing**: Users can spawn cards with randomized colors and artwork.
- **Custom Card Creation**: A text entry box and "+" button in the top-right allow users to add cards with specific labels.
- **Tapping Mechanism**: Double-clicking a card toggles its "tapped" state, rotating the card 90 degrees to indicate it has been used.
- **Deletion**: Selected cards can be removed from the board using the `Delete` or `Backspace` keys.

### 2. Interaction & Manipulation
- **Dragging**: Cards can be moved via left-click and drag.
- **Group Movement**: If multiple cards are selected, they can be dragged as a single unit, maintaining their relative offsets.
- **Grid Snapping**: Upon releasing a dragged card, the system checks if the card is within a specific radius of a grid intersection. If so, the card snaps to that position to help with alignment.
- **Area Selection**: Right-clicking and dragging creates a selection rectangle. All cards intersecting this rectangle are highlighted and grouped for movement or deletion.

### 3. Visual Representation
- **Playmat**: A dark green felt-like background.
- **Card Anatomy**:
    - **Main Color**: A background color representing the card's type/suit.
    - **Artwork**: A 10x10 grid of pixels. The colors of these pixels are randomly jittered around the card's main color to create a cohesive but randomized visual style.
    - **Text Box**: A white area containing the card's label.
    - **Border**: A black border that turns gold when the card is selected.
- **Feedback**: 
    - A semi-transparent rectangle appears during dragging to indicate the nearest snap point.
    - A light blue rectangle indicates the current right-click selection area.

## Architecture

### System Structure
The application follows an immediate-mode GUI pattern where the state is updated and rendered every frame.

- **`main.rs`**: The entry point that configures the native window and initializes the `Playmat` app.
- **`app.rs`**: The heart of the application. It implements `eframe::App` and contains the `update` loop which handles:
    - Input processing (Keyboard/Mouse).
    - State transitions (Dragging, Selecting, Tapping).
    - Rendering logic using the `egui::Painter`.
- **`card.rs`**: Defines the data structures for a `Card` (id, position, color, label, tapped state, artwork) and `DragState` (tracking which cards are being moved and their offsets).
- **`deck.rs`**: Handles the logic for generating new cards, including the color palette and the artwork generation algorithm.

### Data Models
- **`Playmat`**: The global state container.
    - `cards: Vec<Card>`: The list of cards currently on the board.
    - `selected_ids: Vec<u64>`: A list of IDs currently highlighted.
    - `drag: Option<DragState>`: State tracking for active drag operations.
    - `new_card_label: String`: Buffer for the custom card name input.

## Technical Details

### Coordinate System
The app uses `egui::Pos2` for absolute positioning on the screen. Card dimensions are fixed (`CARD_W` x `CARD_H`), and the snapping grid is defined by `GRID_W` and `GRID_H`.

### Z-Ordering
To ensure that the card being dragged always appears on top of others, the application removes dragged cards from the `cards` vector and pushes them to the end of the list during the drag start event.

### Artwork Generation
Artwork is generated as a `Vec<Color32>` of 100 elements. For each pixel, the system takes the base RGB values of the card's main color and adds a random offset (between -60 and +60) to each channel, clamped between 0 and 255.
