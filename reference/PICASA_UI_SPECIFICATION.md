# Picasa Original UI Architecture, Looks & Structure Specification

This document details the reverse-engineered UI architecture, layout hierarchy, design tokens, and assets extracted from the original Google Picasa installer (`picasa39-setup.exe`, v3.9.141.259).

All extracted assets and specification files are preserved locally in [`reference/picasa_original/`](file:///home/pathfindr/dev/pica/reference/picasa_original/).

---

## 1. Engine Architecture & File Breakdown

Picasa’s UI was built upon Lifescape’s proprietary **"Yellowstone"** UI engine:

| Component Type | File / Location | Purpose & Role |
| :--- | :--- | :--- |
| **Yellowstone Respack** | `runtime/respack.yt` | Master UI bundle (3.8MB) storing RLE-compressed RGBA layers, compiled layout trees, and component scripts. |
| **Layout Tree Scripts** | `*.tre` (`tre_text/`) | Declarative UI tree hierarchy specifying parent-child relationships, alignment, constraints, and interactive states. |
| **Modal Window Specs** | `*.fen` | Declarative XML dialog specs for window layouts (`export.fen`, `album.fen`, `options.fen`, etc.). |
| **UI Constants & Metrics** | `runtime/constants.ui` | Global design tokens: colors, font families, font sizes, gutters, padding, and selection borders. |
| **Filter Pipeline** | `runtime/filterdesc.xml` | Complete specification of all non-destructive photo adjustments, sliders, presets, and curve data. |
| **Extracted Graphic Assets** | `ui_assets/` | 1,331 PNG visual elements: button states (normal/hover/press), icons, tabs, sliders, and decorations. |

---

## 2. Design Tokens & Looks

### 2.1. Color Palette

```ini
; Album & Library Navigation
alist_bgcolor        = #F3F3F3   ; Sidebar library background
alist_selcolor_win   = #25648B   ; Selected folder/album (Iconic Picasa Deep Slate Blue)
alist_hicolor_win    = #83A7BD   ; Hover background 1
alist_hicolor2_win   = #E5E2DA   ; Hover background 2
alist_catcolor       = #EDEAE4   ; Category header background
alist_stickycolor    = #EAE7DC   ; Sticky header background
alist_dotcolor       = #BEBEBE   ; Tree dot indicator
alist_dragcolor      = #82A6BD   ; Drag-and-drop indicator

; Folder & Album Banners in Grid
alabel_fldrcol       = #E2E2E2   ; Folder banner background
alabel_fldrhicol     = #F2F2F2   ; Folder banner hover background
alabel_albumcol      = #E2E2E2   ; Album banner background
alabel_albumhicol    = #F2F2F2   ; Album banner hover background

; Thumbnail Grid Selection
thumbsel_color1      = #009EFF   ; Outer glow border (Electric Picasa Blue)
thumbsel_color2      = #FFFFFF   ; Inner border outline (White)
```

### 2.2. Typography

| Role | Font Family | Size | Weight |
| :--- | :--- | :--- | :--- |
| **Album Title** | `Georgia` | 20pt | Regular (`#634B45`) |
| **Album Body / Subhead** | `Georgia` | 14pt | Regular |
| **UI Labels & Captions** | `Praxis Semi Bold/Heavy` | 11pt, 12pt, 14pt | 700 / Bold |
| **Body & Tree Items** | `Praxis LT Regular` | 12pt, 14pt, 16pt, 18pt | 400 / Regular |
| **Big Display / Search** | `Arial Bold` | 24pt | 400 |

### 2.3. Spatial Layout & Metrics

- **Album List (Sidebar)**:
  - Row height: `22px`
  - Tree indentation: `17px` per level
  - Font size: `14px`
- **Thumbnail Grid Canvas**:
  - Outer gutter (`alayout_gutter`): `24px`
  - Horizontal spacing between thumbnails (`alayout_thumbGutterX`): `12px`
  - Vertical spacing between thumbnails (`alayout_thumbGutterY`): `22px`
- **Bottom Persistent Bar**:
  - Total dock height: `~64px`
  - **Picture Tray (Left)**: Spans `0%` to `36.5%` of bottom width (`XConstraint 0, 0, 5` to `XConstraint 1, 0.365, -15`). Contains held thumbnails, "Hold", "Clear", and "Add to..." buttons.
  - **Output Bar (Right)**: Spans remaining width (`36.5%` to `100%`). Button cells: `50px` width × `52px` height each.

---

## 3. UI Structure & Component Hierarchy

### 3.1. Main Window Anatomy (`thumbui`)

```
┌────────────────────────────────────────────────────────────────────────┐
│  [Picasa 3 Header / Menu Bar]                                          │
├───────────────┬──────────────────────────────────────────┬─────────────┤
│ Top Controls  │  [Search Bar & Filter Chips: Star/Face]  │ View Toggle │
├───────────────┼──────────────────────────────────────────┼─────────────┤
│ Navigation    │  Thumbnail Grid Area (`albumsdisplay`)    │ Collapsible │
│ Pane          │                                          │ Metadata    │
│ (`listdecrect`)│  ┌────────────────────────────────────┐  │ Drawer      │
│               │  │ [Folder Header Banner]             │  │             │
│ • Import Btn  │  │  Icon | Name | Date | [Play]       │  │ • People    │
│ • Tree/Flat   │  ├────────────────────────────────────┤  │ • Places    │
│ • Albums      │  │ [Thumb] [Thumb] [Thumb] [Thumb]    │  │ • Tags      │
│ • Folders     │  │ [Thumb] [Thumb] [Thumb] [Thumb]    │  │ • EXIF Info │
│ • Other       │  └────────────────────────────────────┘  │             │
├───────────────┴──────────────────────────────────────────┴─────────────┤
│ Persistent Bottom Dock (`outputlayout` + `scratchback`)                 │
│ ┌───────────────────────────────┬────────────────────────────────────┐ │
│ │ Photo Tray (0 to 36.5% width) │ Output Actions (36.5% to 100%)     │ │
│ │ [Hold] [Clear] [Held Thumbs]  │ [Upload] [Email] [Print] [Export]  │ │
│ │ [Add to Album...]             │ [Shop] [Collage] [Movie] [Zoom ──] │ │
│ └───────────────────────────────┴────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2. Single Image Viewer (`oneup`) & Fullscreen HUD

- **Viewport Canvas**: Centered image (`bicubiccontainer`) with smooth pan & zoom.
- **Translucent Floating HUD (`stripback`)**:
  - Left: `exit` ("Back to Library" button).
  - Center: `prev`, `auto` (Play/Pause Slideshow), `next`, `timeline` (Jump to Timeline).
  - Right: `rotateleft`, `rotateright`, `star` (Toggle favorite), `transtype` (Slideshow transition mode), `captionbutton` (Caption editor).

### 3.3. Non-Destructive Editing Suite (`editpanel`)

Divided into 5 tabs:

1. **Tab 1: Basic Fixes (`basic_icon`)**:
   - `crop`: Interactive crop tool with preset aspect ratios (4:3, 16:9, square, etc.).
   - `straighten` (`horizonadjust`): Leveling slider with grid overlay.
   - `redeye`: Red-eye brush tool with puck cursor.
   - `enhance`: "I'm Feeling Lucky" auto one-click adjustment.
   - `autocontrast` (`autolighting`): Auto contrast stretch.
   - `autocolor`: Auto white-balance adjustment.
   - `filllight`: Fill light exposure boost slider.
   - `edittext`: On-image text overlay tool.
   - `retouch`: Healing / blemish remover clone stamp with circular puck cursor.
2. **Tab 2: Tuning (`tuning_icon`)**:
   - Fill Light slider
   - Highlights slider
   - Shadows slider
   - Color Temperature slider + neutral grey dropper (`dropper_n` / `dropper_h`)
3. **Tabs 3, 4, 5: Creative Effects**:
   - 36 effects arranged in 3×4 or 2×6 thumbnail grid buttons (`fxpreview1`..`fxpreview12`).
   - Includes B&W, Sepia, Warmify, Vignette, Soft Focus, Film Grain, Tint, Cinema, Holga, HDR, Lomo, etc.
   - Comparison buttons: `aa_2up_icon` (side-by-side original/original) and `ab_2up_icon` (side-by-side before/after).

---

## 4. Summary of Assets Extracted

In [`reference/picasa_original/ui_assets/`](file:///home/pathfindr/dev/pica/reference/picasa_original/ui_assets/):
- **`icons/`**: 78 tree items (folders, starred folders, face tags, camera imports, web albums).
- **`thumbui/`**: Grid controls, star toggles, folder/flat view switches, rotate buttons, tray buttons.
- **`outputlayout/`**: All bottom tray action icons (Print, Email, Export, Order, Blogger, Collage, Movie, More).
- **`editpanel/`**: All editing tools, sliders, tabs, comparison icons, and effect button templates.
- **`slideshowctrls/`**: Fullscreen HUD controls and playback buttons.
- **`globalbuttons/`**: 269 UI controls (normal, pressed, hovered checkboxes, radio buttons, bevels).
