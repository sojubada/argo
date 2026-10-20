# webdoc

JavaScript renderers, bundled so the Apple TV never has to reach the
network to open a document. Copied from npm tarballs, unmodified.

| file | package | version | licence |
|---|---|---|---|
| jszip.min.js | jszip | 3.10.2 | MIT / GPLv3 |
| docx-preview.min.js | docx-preview | 0.4.1 | Apache-2.0 |

These exist because the tvOS SDK ships no WebKit, but the device does:
the probe in Spartan/Diagnostics reached it through dlopen, loaded a
page, ran JavaScript and came back with pixels. Everything in this
folder runs inside that web view.
