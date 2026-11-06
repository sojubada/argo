# Argo

A file browser and document/media viewer for Apple TV (tvOS 16, TrollStore).
Built on [Spartan](https://github.com/WhitetailAni/Spartan), with the goal of a "small PC inside the TV" that works with just the remote and a keyboard.

## Features

| Area | What it does |
|---|---|
| Files | Dual-pane browser, sort and filter, copy/move/trash, system-folder protection, app names shown on app container folders |
| Keyboard | Bluetooth keyboard shortcuts (⌫ parent folder, ⌘⌫ root, Delete to trash, etc.), Korean input |
| Remote | WebDAV and FTP, download and open, install IPAs from a NAS straight into TrollStore |
| Documents | PDF (two-page view, night mode), Hangul hwp/hwpx, Word doc/docx, Excel xls/xlsx, PowerPoint ppt/pptx, text and code editing |
| Fonts | Renders documents with fonts from the font folder, falling back to the closest match |
| Music | Apple Music–style screen, synced lyrics (LRC), album art lookup, spectrum bar, photo slideshow |
| Video | Native Apple TV player, remote and keyboard controls |
| Look | 6 themes and app icons |

Documents are converted by a Rust engine inside the app (`rust/argoconv`): it renders each page to SVG, bundles the pages into a PDF, and draws that PDF with CoreGraphics.
This approach is needed because tvOS has no PDFKit, WebKit or QuickLook.

## Install

1. Download an IPA from [Releases](../../releases) (unsigned).
   - `Argo-Safe.ipa`: safe edition. Safe mode is always on, and folders containing locked items cannot be deleted.
   - `Argo-Adventurer.ipa`: adventurer edition. Safe mode can be turned off.
2. Install with TrollStore.

Builds run on GitHub Actions (macOS 15, Xcode 16). Changes and how to revert them are in [`docs/RELEASE_2026-10-30.md`](docs/RELEASE_2026-10-30.md) (Korean).

## Upstream sources

| Source | Used for | License |
|---|---|---|
| [WhitetailAni/Spartan](https://github.com/WhitetailAni/Spartan) | App base (file manager) | MIT |
| [WhitetailAni/PrivateKits-tvOS](https://github.com/WhitetailAni/PrivateKits-tvOS) | Installed app list (ApplicationsWrapper) | See repository |
| [marmelroy/Zip](https://github.com/marmelroy/Zip) | Unzipping | MIT |
| [opa334/TrollStore](https://github.com/opa334/TrollStore) | Root helper (TSUtil) | MIT |
| Pogo (Amy While) | Command execution (CommandRunner) | Attribution to original author |
| SwiftTryCatch (William Falcon) | Objective-C exception handling | MIT |
| [edwardkim/rhwp](https://github.com/edwardkim/rhwp) | Hangul hwp/hwpx rendering, EMF/WMF conversion | MIT |
| [Indosaram/hwpers](https://github.com/Indosaram/hwpers) | Fallback Hangul engine | MIT / Apache-2.0 |
| [mini-software/MiniPdf](https://github.com/mini-software/MiniPdf) | Word/Excel → PDF | Apache-2.0 |
| [resvg family](https://github.com/linebender/resvg) (usvg, svg2pdf, pdf-writer, ttf-parser, rustybuzz) | SVG, PDF and font handling | MIT / Apache-2.0 |
| ECMA-376 presetShapeDefinitions | PowerPoint shape definitions | ECMA standard |
| [iTunes Search API](https://performance-partners.apple.com/search-api), [LRCLIB](https://lrclib.net) | Album art and lyrics lookup (can be turned off) | Each service's terms |

The PowerPoint renderer (`rust/argoconv/src/slides`) and the tvOS screens were written from scratch in this repository.
License notices for vendored sources are kept as-is in each folder's LICENSE and Cargo.toml (`rust/vendor/*`).

## License

MIT. Original Spartan © 2023 WhitetailAni. Vendored components follow their own licenses.
