# Argo

A tvOS file browser with **WebDAV support** and **TrollStore IPA installation**, for
Apple TV. Forked from [Spartan](https://github.com/WhitetailAni/Spartan)
(MIT, © 2023 WhitetailAni).

Argo was the ship that sailed out to fetch something worth having. That is roughly
what this app does: it reaches a NAS or a server over WebDAV, brings files back to
the Apple TV, and hands IPAs to TrollStore.

## What this fork adds

- **WebDAV connections** — save several servers (URL, username, password, optional
  "allow untrusted certificates" for self-signed NAS boxes) and browse them from
  the TV.
- **Download and view** — any remote file can be pulled into a local cache and
  opened with the existing viewers (text, hex, image, video, audio).
- **Install IPAs from WebDAV** — `.ipa` / `.tipa` files are downloaded and handed to
  TrollStore, so an IPA library can live on a NAS and be installed from the couch.
- **Unsigned IPA builds via GitHub Actions** — no local Xcode needed.
- **New identity** — Argo name, icon and top shelf artwork, and its own bundle
  identifier (`com.sojubada.argo`), so it installs alongside Spartan rather than
  replacing it.

The WebDAV button (globe icon) is in the main toolbar, next to the mount-points
button.

## How installing an IPA works

Two routes, tried in this order:

1. **TrollStore's root helper.** Argo locates `trollstorehelper` inside the installed
   TrollStore bundle and runs `trollstorehelper install custom <path>` as root, using
   the same persona-spawn mechanism TrollStore itself uses. No user interaction.
2. **TrollStore's URL scheme.** If the helper cannot be found, Argo falls back to
   `apple-magnifier://install?url=…`, which makes TrollStore fetch the IPA itself.
   This needs "URL Scheme" enabled in TrollStore settings, and the URL has to be
   reachable by TrollStore (credentials included).

The connection screen shows which route is currently available.

## Implementation notes

- `Spartan/WebDAV/WebDAVClient.swift` is a dependency-free WebDAV client built on
  `URLSession` + `XMLParser` (PROPFIND / GET / PUT / MKCOL / DELETE / MOVE). It is
  completion-handler based so it runs on the tvOS 13 deployment target without the
  Swift concurrency back-deployment libraries.
- Downloads are staged in `/private/var/mobile/Documents/Argo`.
- `Info.plist` carries `NSAppTransportSecurity` (plain-HTTP NAS boxes),
  `NSLocalNetworkUsageDescription` (tvOS 16+ local network privacy) and
  `LSApplicationQueriesSchemes` for the TrollStore scheme.
- Artwork is generated, not hand-drawn: three parallax layers for the app icon
  (sky, sea, ship) plus the top shelf images, all in the asset catalog.
- The entitlements the upstream project injects in a build-phase script are also
  kept in `Argo.entitlements`, so CI can apply them with `ldid` to an unsigned
  build. That build-phase script runs under python3 and skips cleanly when no
  `.xcent` is produced.
- The Xcode project and source folder are still named `Spartan` internally; only
  the product is renamed. This keeps the diff against upstream readable.

## Building

Push to `main`, or run the **Build unsigned IPA** workflow by hand. It:

1. selects Xcode 15,
2. replaces `spawn.h` in the tvOS SDK with the one in this repo (upstream requires
   this; the app calls `posix_spawnattr_set_persona_np`),
3. builds unsigned, fakesigns with `ldid` using `Argo.entitlements`,
4. packages `Argo.ipa` and publishes it as a release asset.

Install the resulting IPA with TrollStore on the Apple TV.

## License

MIT, inherited from upstream. See `LICENSE`.
