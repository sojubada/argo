# webdoc

JavaScript renderers, bundled so the Apple TV never has to reach the
network to open a document.

| file | package | version | licence | global |
|---|---|---|---|---|
| jszip.min.js | jszip | 3.10.2 | MIT | `JSZip` |
| docx-preview.min.js | docx-preview | 0.4.1 | Apache-2.0 | `docx` |
| xlsx.min.js | xlsx (SheetJS) | 0.18.5 | Apache-2.0 | `XLSX` |
| pptx.min.js | pptx-preview | 1.0.7 | ISC | `pptxPreview` |
| hwp.min.js | hwp.js | 0.0.3 | Apache-2.0 | `hwpjs` |
| pdf.min.js | pdfjs-dist (legacy) | 3.11.174 | Apache-2.0 | `pdfjsLib` |
| pdf.worker.min.js | pdfjs-dist (legacy) | 3.11.174 | Apache-2.0 | (worker) |

jszip, docx-preview and xlsx ship browser builds and are copied from
the npm tarball unmodified. pptx-preview and hwp.js ship only ES
modules against Node built-ins, so they were bundled with esbuild into
IIFEs with `fs`, `stream`, `events`, `buffer` and `util` stubbed —
nothing in the rendering path uses them.

These exist because the tvOS SDK ships no WebKit, but the device does.
A .docx of 52 KB renders in 0.89 seconds with its highlighting intact,
which is the fidelity that re-typesetting the text was always going to
lose.

The legacy PDF.js build is the one that targets older browsers; the
current build assumes JavaScript newer than this WebKit has.
