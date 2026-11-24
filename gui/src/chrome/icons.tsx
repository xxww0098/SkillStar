import { Canvas, Path2D, type JSX } from "nsh";

// Icon glyphs for the shell and chrome, drawn as stroked paths on a 24×24
// grid (Lucide conventions: 2px round strokes, no fill) so a single draw()
// can tint them any palette color — the GPUI side reads the same shapes from
// the kit's Lucide asset set (nav.rs:75-86). The four skill-mode nav glyphs
// (sparkle/store/deck-stack/briefcase) must stay mutually distinct in
// silhouette: collapsed mode shows them icon-only.
//
// Waves replace individual entries with the migrated SVG assets when the
// icon-asset wave lands; the call shape `Icon({name,size,color})` stays.

export type IconName =
  | "sparkle"
  | "store"
  | "deck"
  | "briefcase"
  | "users"
  | "settings"
  | "panelLeftClose"
  | "panelLeftOpen"
  | "sun"
  | "moon"
  | "search"
  | "refresh"
  | "arrowUp"
  | "plus"
  | "check"
  | "x"
  | "info"
  | "checkCircle"
  | "alertTriangle"
  | "xCircle"
  | "circleAlert"
  | "layoutGrid"
  | "layers"
  | "download"
  | "list"
  | "share"
  | "ellipsis"
  | "copy"
  | "trash"
  | "clipboard"
  | "package"
  | "loader"
  | "rotateCcw"
  | "checkBig"
  | "link"
  | "pencil"
  | "qrCode"
  | "packageOpen"
  | "star"
  | "chevronLeft"
  | "chevronRight"
  | "folder"
  | "arrowLeft"
  | "badgeCheck"
  | "laptop"
  | "server"
  | "chevronDown"
  | "chevronUp"
  | "arrowUpDown"
  | "listFilter"
  | "circleArrowUp"
  | "gitBranch"
  | "hardDrive"
  | "square"
  | "squareCheck"
  | "squareMinus"
  | "externalLink"
  // Settings-section glyphs (settings/mod.rs SettingsSection::icon — the
  // Lucide names the GPUI rail uses; `link` already exists above), added
  // by the settings-base wave.
  | "globe"
  | "zap"
  | "activity"
  | "eyeOff"
  | "languages"
  | "fileText"
  // Projects-page glyphs (projects/mod.rs FolderOpen rail glyph, detail.rs
  // Scan rules header), added by the projects wave.
  | "folderOpen"
  | "scan"
  | "terminal"
  // GitHub-mirror section glyphs (settings/gh_mirror.rs ArrowDown / Wifi /
  // WifiOff — the reorder column and the test-probe verdict icons), added
  // by the settings-network slice.
  | "arrowDown"
  | "wifi"
  | "wifiOff"
  // Settings agents/storage/about glyphs (agent_connections.rs Link2Off,
  // storage/{rows,parts}.rs Database / FolderGit2 / FileClock /
  // Stethoscope / Wrench / Eraser), added by the settings-sections-2 wave.
  | "link2Off"
  | "database"
  | "folderGit2"
  | "fileClock"
  | "stethoscope"
  | "wrench"
  | "eraser"
  | "eye"
  | "play"
  | "rows3"
  | "clock"
  | "bot"
  | "sliders"
  | "gauge";

const PATHS: Record<IconName, string[]> = {
  // Four-point star with two sparks — “one skill”.
  sparkle: [
    "M12 3l1.9 5.7a2 2 0 0 0 1.4 1.4L21 12l-5.7 1.9a2 2 0 0 0-1.4 1.4L12 21l-1.9-5.7a2 2 0 0 0-1.4-1.4L3 12l5.7-1.9a2 2 0 0 0 1.4-1.4L12 3z",
    "M19 3v2",
    "M20 4h-2",
  ],
  // Storefront: awning scallops, body, door.
  store: [
    "M3 8l2-4h14l2 4",
    "M3 8c0 1.7 1.3 3 3 3s3-1.3 3-3c0 1.7 1.3 3 3 3s3-1.3 3-3c0 1.7 1.3 3 3 3s3-1.3 3-3",
    "M5 11v9h14v-9",
    "M10 20v-5h4v5",
  ],
  // Deck: two offset stacked cards.
  deck: ["M8.5 3H20v8.5H8.5z", "M4 13.5h11.5V22H4z"],
  // Briefcase: box, handle, belt line.
  briefcase: ["M3 8h18v12H3z", "M9 8V6a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2", "M3 13h18"],
  users: [
    "M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0z",
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
    "M22 21v-2a4 4 0 0 0-3-3.9",
    "M16 3.1a4 4 0 0 1 0 7.8",
  ],
  // Gear (Lucide settings): toothed cogwheel plus center axle ring.
  settings: [
    "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
    "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
  ],
  panelLeftClose: ["M3 4h18v16H3z", "M9 4v16", "M16 9l-3 3 3 3"],
  panelLeftOpen: ["M3 4h18v16H3z", "M9 4v16", "M11 9l3 3-3 3"],
  sun: [
    "M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0z",
    "M12 2v2",
    "M12 20v2",
    "M2 12h2",
    "M20 12h2",
    "M4.9 4.9l1.4 1.4",
    "M17.7 17.7l1.4 1.4",
    "M19.1 4.9l-1.4 1.4",
    "M6.3 17.7l-1.4 1.4",
  ],
  moon: ["M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"],
  search: ["M10.5 18a7.5 7.5 0 1 1 0-15 7.5 7.5 0 0 1 0 15z", "M21 21l-5.2-5.2"],
  refresh: ["M20 5v5h-5", "M19.4 10a8 8 0 1 0 .6 4"],
  arrowUp: ["M12 19V5", "M5 12l7-7 7 7"],
  // Lucide's plus spans 14 of the 24 grid; widened to 16 (y/x 4..20) so its ink
  // matches refresh (4..20) — at BAR_ICON 14 the two glyphs then read the same
  // optical size beside each other in a toolbar row.
  plus: ["M12 4v16", "M4 12h16"],
  check: ["M20 6L9 17l-5-5"],
  x: ["M18 6L6 18", "M6 6l12 12"],
  info: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M12 16v-4", "M12 8h.01"],
  checkCircle: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M8 12l3 3 5-5"],
  alertTriangle: ["M12 3l9.5 17H2.5L12 3z", "M12 10v4", "M12 17.2h.01"],
  xCircle: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M15 9l-6 6", "M9 9l6 6"],
  // circle-alert (Lucide): the duplicate-name warning in the deck editor.
  circleAlert: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M12 8v4", "M12 16h.01"],
  layoutGrid: ["M4 4h7v7H4z", "M13 4h7v7h-7z", "M4 13h7v7H4z", "M13 13h7v7h-7z"],
  // Deck-card glyphs (skill_cards/group_card.rs + mod.rs toolbar), Lucide
  // shapes on the same 24×24 stroke grid.
  layers: ["M12 2l10 5-10 5L2 7l10-5z", "M2 12l10 5 10-5", "M2 17l10 5 10-5"],
  download: ["M12 3v12", "M7 10l5 5 5-5", "M5 21h14"],
  list: ["M8 6h13", "M8 12h13", "M8 18h13", "M3 6h.01", "M3 12h.01", "M3 18h.01"],
  share: [
    "M18 8a3 3 0 1 1 0-6 3 3 0 0 1 0 6z",
    "M6 15a3 3 0 1 1 0-6 3 3 0 0 1 0 6z",
    "M18 22a3 3 0 1 1 0-6 3 3 0 0 1 0 6z",
    "M8.6 13.5l6.8 4",
    "M15.4 6.5l-6.8 4",
  ],
  ellipsis: ["M5 12h.01", "M12 12h.01", "M19 12h.01"],
  copy: ["M9 9h11v11H9z", "M5 15H4V4h11v1"],
  trash: ["M4 7h16", "M10 11v6", "M14 11v6", "M6 7l1 13h10l1-13", "M9 7V4h6v3"],
  // Deck-import dialog glyphs (skill_cards/deck_import.rs): Clipboard
  // (input prefix), Package (pick-file action), Loader (spinner well),
  // RotateCcw (retry), CircleCheckBig (completed well). Lucide shapes on
  // the same 24×24 stroke grid.
  clipboard: [
    "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2",
    "M9 2h6a1 1 0 0 1 1 1v2a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z",
  ],
  package: [
    "M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z",
    "M12 22V12",
    "M3.3 7l7.703 4.734a2 2 0 0 0 1.994 0L20.7 7",
    "M7.5 4.27l9 5.15",
  ],
  loader: ["M21 12a9 9 0 1 1-6.219-8.56"],
  rotateCcw: [
    "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.874 2.613L3 8",
    "M3 3v5h5",
  ],
  checkBig: ["M21.801 10A10 10 0 1 1 17 3.335", "M9 11l3 3L22 4"],
  // Deck-drawer action glyphs (skill_cards/detail_drawer.rs): Link (the
  // deploy-all command), Pencil (edit deck). Lucide shapes, same grid.
  link: [
    "M9 17H7A5 5 0 0 1 7 7h2",
    "M15 7h2a5 5 0 1 1 0 10h-2",
    "M8 12h8",
  ],
  pencil: ["M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"],
  // Share-sheet section glyphs (skill_cards/share_sheet.rs): QrCode (the
  // code section) and PackageOpen (the bundle section), Lucide shapes on
  // the same 24×24 stroke grid.
  qrCode: [
    "M4 4h5v5H4z",
    "M15 4h5v5h-5z",
    "M4 15h5v5H4z",
    "M14 14h2v2h-2z",
    "M18 18h2v2h-2z",
    "M14 18h2",
    "M18 14h2",
  ],
  packageOpen: [
    "M3.5 9L6 4.5h12L20.5 9",
    "M3.5 9h17",
    "M5.5 9v10a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1V9",
    "M12 13V6",
    "M9.5 8.5L12 6l2.5 2.5",
  ],
  // Marketplace glyphs (market_card.rs + card.rs + view.rs): Star (the
  // download/stars evidence line), ChevronRight (publisher row affordance),
  // Folder (publisher repo count). Lucide shapes, same 24×24 stroke grid.
  star: [
    "M12 2l2.9 6.26 6.86.83-5.08 4.7 1.34 6.77L12 17.2l-6.02 3.36 1.34-6.77-5.08-4.7 6.86-.83L12 2z",
  ],
  chevronLeft: ["M15 18l-6-6 6-6"],
  chevronRight: ["M9 18l6-6-6-6"],
  folder: [
    "M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-8l-2-3H4a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1z",
  ],
  arrowLeft: ["M19 12H5", "M12 19l-7-7 7-7"],
  badgeCheck: [
    "M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1-4.78-4.77 4 4 0 0 1 0-6.76Z",
    "m9 12 2 2 4-4",
  ],
  // My-skills page glyphs (my_skills/toolbar.rs + origin_menu.rs +
  // sort_menu.rs + empty_state.rs + skill_card/render.rs). Lucide shapes on
  // the same 24×24 stroke grid; the icon-asset wave swaps them for SVG
  // assets without touching call sites.
  laptop: ["M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9m16 0H4m16 0 1.5 4h-17L4 16"],
  server: [
    "M2 4a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4z",
    "M2 16a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4z",
    "M6 6h.01",
    "M6 18h.01",
  ],
  chevronDown: ["m6 9 6 6 6-6"],
  chevronUp: ["m18 15-6-6-6 6"],
  arrowUpDown: ["m21 16-4 4-4-4", "M17 20V4", "m3 8 4-4 4 4", "M7 4v16"],
  listFilter: ["M3 6h18", "M7 12h10", "M10 18h4"],
  circleArrowUp: [
    "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z",
    "m16 12-4-4-4 4",
    "M12 16V8",
  ],
  gitBranch: [
    "M6 3v12",
    "M18 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    "M6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    "M18 9a9 9 0 0 1-9 9",
  ],
  hardDrive: [
    "M2 12h20",
    "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
    "M6 16h.01",
    "M10 16h.01",
  ],
  square: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"],
  squareCheck: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "m9 12 2 2 4-4"],
  squareMinus: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M9 12h6"],
  // dialog_chrome.rs ExternalLink；Lucide 24×24 stroke 形。
  externalLink: ["M15 3h6v6", "M10 14 21 3", "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"],
  // Settings-section glyphs — Lucide paths, same conventions as above
  // (settings/mod.rs SettingsSection::icon parity; `link` reuses the glyph
  // at the top of this table).
  globe: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M3 12h18", "M12 3a14.5 14.5 0 0 1 0 18", "M12 3a14.5 14.5 0 0 0 0 18"],
  zap: ["M13 2L3 14h9l-1 8 10-12h-9l1-8z"],
  activity: ["M22 12h-4l-3 9L9 3l-3 9H2"],
  eyeOff: [
    "M9.88 9.88a3 3 0 1 0 4.24 4.24",
    "M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68",
    "M6.61 6.61A13.526 13.526 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61",
    "M2 2l20 20",
  ],
  languages: ["M5 8l6 6", "M4 14l6-6 2-3", "M2 5h12", "M7 2h1", "M22 22l-5-10-5 10", "M14 18h6"],
  fileText: [
    "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z",
    "M14 2v4a2 2 0 0 0 2 2h4",
    "M16 13H8",
    "M16 17H8",
    "M10 9H8",
  ],
  terminal: ["M4 17l6-6-6-6", "M12 19h8"],
  // Projects page (projects/): FolderOpen (the selected rail card and the
  // detail header badge) and Scan (the detected-rules card header), Lucide
  // shapes on the same 24×24 stroke grid.
  folderOpen: [
    "M6 14l1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2",
  ],
  scan: [
    "M3 7V5a2 2 0 0 1 2-2h2",
    "M17 3h2a2 2 0 0 1 2 2v2",
    "M21 17v2a2 2 0 0 1-2 2h-2",
    "M7 21H5a2 2 0 0 1-2-2v-2",
    "M7 12h10",
  ],
  // GitHub-mirror section (settings/gh_mirror.rs): the reorder column's
  // second arrow and the test-probe verdict glyphs — Lucide shapes, same
  // 24×24 stroke grid (`arrowUp` above is the first half of the column).
  arrowDown: ["M12 5v14", "M19 12l-7 7-7-7"],
  wifi: [
    "M5 12.55a11 11 0 0 1 14.08 0",
    "M1.42 9a16 16 0 0 1 21.16 0",
    "M8.53 16.11a6 6 0 0 1 6.95 0",
    "M12 20h.01",
  ],
  wifiOff: [
    "M12 20h.01",
    "M8.5 16.429a5 5 0 0 1 7 0",
    "M5 12.859a10 10 0 0 1 5.17-2.69",
    "M19 12.859a10 10 0 0 1 2.007-1.523",
    "M2 8.82a15 15 0 0 1 4.177-2.643",
    "M22 8.82a15 15 0 0 0-11.288-3.764",
    "M2 2l20 20",
  ],
  // Settings agents/storage/about sections (agent_connections.rs,
  // storage/{rows,parts}.rs, about.rs) — Lucide shapes on the same 24×24
  // stroke grid; the icon-asset wave swaps them for SVG assets later.
  link2Off: [
    "M9 17H7A5 5 0 0 1 7 7h2",
    "M15 7h2a5 5 0 1 1 0 10h-2",
    "M8 12h8",
    "M2 2l20 20",
  ],
  database: [
    "M12 8c4.97 0 9-1.343 9-3s-4.03-3-9-3-9 1.343-9 3 4.03 3 9 3z",
    "M21 12c0 1.66-4 3-9 3s-9-1.34-9-3",
    "M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5",
  ],
  folderGit2: [
    "M9 20H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4l2 3h9a2 2 0 0 1 2 2v8",
    "M12 12a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    "M12 9v3",
    "M9 18h6",
  ],
  fileClock: [
    "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z",
    "M14 2v4a2 2 0 0 0 2 2h4",
    "M16 14.5a4 4 0 1 0-8 0 4 4 0 0 0 8 0z",
    "M16 14.5h-2v-2",
  ],
  stethoscope: [
    "M4.8 2.3A.3.3 0 1 0 5 2H4a2 2 0 0 0-2 2v5a6 6 0 0 0 6 6 6 6 0 0 0 6-6V4a2 2 0 0 0-2-2h-1a.2.2 0 1 0 .3.3",
    "M8 15v1a6 6 0 0 0 6 6 6 6 0 0 0 6-6v-4",
    "M2 3v.01",
    "M18 12a2 2 0 1 0 0-.01",
  ],
  wrench: [
    "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z",
  ],
  eraser: [
    "m7 21-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21",
    "M22 21H7",
    "m5 11 9 9",
  ],
  // 账号工作台新增字形（工具栏、视图切换、隐私开关、最近使用时间）。
  eye: [
    "M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z",
    "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
  ],
  play: ["M6 3l14 9-14 9V3z"],
  rows3: ["M3 4h18v4H3z", "M3 10h18v4H3z", "M3 16h18v4H3z"],
  clock: ["M12 21a9 9 0 1 1 0-18 9 9 0 0 1 0 18z", "M12 7v5l3 2"],
  bot: [
    "M12 8V4H8",
    "M6 8h12a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z",
    "M2 14h2",
    "M20 14h2",
    "M15 13v2",
    "M9 13v2",
  ],
  sliders: [
    "M21 4h-7",
    "M10 4H3",
    "M21 12h-9",
    "M8 12H3",
    "M21 20h-5",
    "M12 20H3",
    "M14 2v4",
    "M8 10v4",
    "M16 18v4",
  ],
  // Lucide gauge: 仪表盘半圆弧与指针，用于侧栏底栏与设置页额度快捷入口。
  gauge: [
    "m12 14 4-4",
    "M3.34 19a10 10 0 1 1 17.32 0",
  ],
};

const cache = new Map<string, Path2D>();

function pathOf(d: string): Path2D {
  let p = cache.get(d);
  if (!p) {
    p = new Path2D(d);
    cache.set(d, p);
  }
  return p;
}

// Sized, colored icon — chrome::icon parity (chrome/mod.rs:137-146).
export function Icon(props: { name: IconName; size?: number; color: string }): JSX.Element {
  return (
    <Canvas
      style={{
        width: props.size ?? 16,
        height: props.size ?? 16,
        flexShrink: 0,
      }}
      draw={(cx) => {
        const scale = (props.size ?? 16) / 24;
        cx.scale(scale, scale);
        cx.strokeStyle = props.color;
        cx.lineWidth = 2;
        cx.lineCap = "round";
        cx.lineJoin = "round";
        for (const d of PATHS[props.name]) {
          cx.stroke(pathOf(d));
        }
      }}
    />
  );
}
