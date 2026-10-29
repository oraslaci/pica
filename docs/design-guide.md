# 🎨 Pica Design Guide (`docs/design-guide.md`)

> **TL;DR:** Pica combines the beloved soul and muscle-memory of **Google Picasa 3.9** with modern, high-contrast darkroom precision. Every screen across the **Library**, **Non-Destructive Editor**, and **Standalone Photo Viewer** must follow this guide so the entire suite feels like a single, cohesive, eye-candy experience.

---

## 🌟 1. Core Design Principles

1. **Photo-First Darkroom Immersion**
   - In viewing and editing modes, chrome must never compete with the photography.
   - The viewport uses a deep obsidian backdrop (`#0e0e11`) enhanced by a subtle, blurred **ambient color glow** sampled from the active photo, framing the image with a crisp micro-border (`ring-1 ring-white/10`) and deep elevation shadow (`shadow-[0_24px_80px_rgba(0,0,0,0.85)]`).
2. **Frameless Viewer & Floating Glass Pills ("Picasa HUDs")**
   - The standalone **Pica Photo Viewer** runs **borderless** (`decorations: false` in Tauri) with **no full-width top title bar**.
   - Controls live in translucent floating glass pills (`bg-[#18181e]/90 border border-[#3f3f4c]/80 backdrop-blur-xl shadow-2xl`) that hover gracefully above the canvas.
3. **Zero-Latency Tactile Physics**
   - Every interaction gives `<16ms` visual feedback.
   - **90° Rotations** glide physically using a monotonic angle counter and a `380ms cubic-bezier(0.2, 0.9, 0.3, 1)` spring curve (never snapping or spinning backward across `360°`).
   - **Mouse-Wheel Zooming** uses exponential scaling (`Math.exp(-e.deltaY * 0.002)`) anchored to the exact pixel under the cursor.
4. **One Visual Language Across All Modes**
   - The Library, Editor (`ImageViewer.svelte`), and Standalone Viewer (`PicaPhotoViewer.svelte`) share the exact same color tokens, button radii, Picasa shutter emblem, `Histogram.svelte` widget, and `PropertiesSidebar.svelte` ("Info for Nerds") drawer.

---

## 🎨 2. Color & Surface Token System

| Surface / Role | Hex / Tailwind Token | Usage |
| :--- | :--- | :--- |
| **Darkroom Viewport** | `#0e0e11` | Main photo canvas background in Editor and Standalone Viewer |
| **App Frame / Shell** | `#121214` / `#181818` | Outer window frame and Library grid canvas |
| **Floating Glass HUD** | `bg-[#18181e]/90 border-[#3f3f4c]/80` | Bottom control dock, floating corner pills, and tool bars (`backdrop-blur-xl`) |
| **Inspector / Sidebar** | `bg-[#1c1c23] border-[#2e2e38]` | Left Editor suite and right `PropertiesSidebar` ("Info for Nerds") |
| **Section Header Bar** | `bg-[#23232c] border-[#2e2e38]` | Drawer headers and collapsible tool headers |
| **Picasa Primary Blue** | `#3b82f6` (`bg-blue-600 hover:bg-blue-500`) | Primary CTAs (`Import`, `Save to Disk`, `Edit in Pica`), active mode pills (`1-Up`, `Fit`, `1:1`), and active filmstrip ring |
| **Active Pill Badge** | `bg-blue-600/30 text-blue-300 border-blue-500/40` | Zoom percentage readout, active filter chips, and `Info for Nerds` badge |
| **Starred / Slideshow Gold** | `#fbbf24` (`text-amber-400 fill-amber-400`) | Favorite star icon and active slideshow indicator |
| **Original Safe Emerald** | `#34d399` (`text-emerald-400`) | `.pica_originals` backup badge and copy confirmation checkmarks |

---

## 💠 3. Brand Emblem (The Picasa 6-Petal Shutter)

Whenever Pica branding appears (Library `TopBar`, Viewer floating identity pill, or the **"Edit in Pica"** handoff button), always use the canonical 6-petal colorful shutter SVG:

```svelte
<svg viewBox="0 0 100 100" class="w-5 h-5 drop-shadow-xs shrink-0">
  <circle cx="50" cy="50" r="48" fill="#1e293b" />
  <path d="M50 10 A40 40 0 0 1 90 50 L50 50 Z" fill="#ef4444" />
  <path d="M90 50 A40 40 0 0 1 78 84 L50 50 Z" fill="#f97316" />
  <path d="M78 84 A40 40 0 0 1 32 87 L50 50 Z" fill="#eab308" />
  <path d="M32 87 A40 40 0 0 1 12 59 L50 50 Z" fill="#22c55e" />
  <path d="M12 59 A40 40 0 0 1 25 21 L50 50 Z" fill="#3b82f6" />
  <path d="M25 21 A40 40 0 0 1 50 10 L50 50 Z" fill="#a855f7" />
  <circle cx="50" cy="50" r="18" fill="#0f172a" />
  <circle cx="50" cy="50" r="14" fill="#38bdf8" opacity="0.6" />
</svg>
```

---

## 🔤 4. Typography & Readout Rules

- **UI Controls & Labels:** Geometric system sans (`font-sans`), `text-xs` (`12px`) or `text-[11px]`, `font-medium` to `font-semibold`, `text-neutral-200`.
- **Technical Readouts:** Always use `font-mono` for:
  - Photo position counters (`14 / 426`)
  - Zoom percentages (`100%`, `240%`)
  - Straighten/rotation angles (`+1.5°`, `90°`)
  - EXIF camera specs (`1/250s`, `f/2.8`, `ISO 400`, `35mm`)
  - Keyboard shortcut hints (`(Esc)`, `(Enter)`, `(Space)`)

---

## 🪟 5. Component Architecture Across Editor & Viewer

1. **Bottom Center Floating HUD Dock:**
   - Rounded pill (`rounded-full`) with `bg-[#18181e]/90 border border-[#3f3f4c]/80 shadow-2xl backdrop-blur-xl`.
   - Icon buttons use `p-2 rounded-full hover:bg-white/10 text-neutral-300 hover:text-white transition-colors cursor-pointer`.
   - Vertical dividers use `w-px h-5 bg-neutral-700/60 mx-0.5`.
2. **Floating Filmstrip Ribbon (Viewer):**
   - Sits directly beneath the floating HUD pill as a sleek rounded glass tray (`rounded-2xl bg-[#16161c]/90 border border-[#333340]/85 backdrop-blur-xl shadow-2xl px-3 py-2`).
   - Active thumbnail uses `ring-2 ring-blue-500 border-blue-400 scale-105 shadow-lg`.
3. **Histogram & Properties Drawer:**
   - Both the Editor and Standalone Viewer use `<Histogram />` (bottom-left floating card) and `<PropertiesSidebar />` (right-hand inspector drawer) so users get the exact same pro-grade inspection tools in both windows.
