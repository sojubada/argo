# 5단계 참고 코드

제미나이에게 그대로 전달하십시오. 아래는 **현재 저장소에 들어 있는 실제 코드**입니다.
`바꾸기 전` 블록을 만들 때 여기서 글자 그대로 복사하십시오 — 공백과 탭이 섞여 있습니다.

## 1. `Spartan/Diagnostics/DocPipelineProbe.swift` (전체)

tvOS 16 SDK에서 **컴파일이 확인된** 호출들입니다. 이 형태를 그대로 따르십시오.

```swift
//
//  DocPipelineProbe.swift
//  Argo
//
//  COMPILE-ONLY PROBE. Nothing calls these functions.
//  Their only job is to prove, on the real tvOS SDK in CI, that every
//  API the document-to-PDF pipeline is planned around actually exists
//  before a line of the feature is specified. PDFKit taught us that
//  Apple's documentation is not evidence; a green build is.
//
//  Delete this file once DocumentPDFBuilder ships.
//

import Foundation
import UIKit
import CoreText
import CoreGraphics
import os

enum DocPipelineProbe {

    // MARK: - 1. CoreText: can we lay out and paginate attributed text?

    static func probeCoreTextPagination(_ text: String, pageSize: CGSize) -> Int {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 28),
            .foregroundColor: UIColor.black,
            .paragraphStyle: NSParagraphStyle.default
        ]
        let attributed = NSAttributedString(string: text, attributes: attrs)
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)

        var pageCount = 0
        var location = 0
        let total = attributed.length
        let bounds = CGRect(origin: .zero, size: pageSize)

        while location < total && pageCount < 10_000 {
            let path = CGPath(rect: bounds, transform: nil)
            let frame = CTFramesetterCreateFrame(
                framesetter,
                CFRangeMake(location, 0),
                path,
                nil
            )
            let visible = CTFrameGetVisibleStringRange(frame)
            if visible.length <= 0 { break }
            location += visible.length
            pageCount += 1
        }
        return pageCount
    }

    static func probeSuggestedSize(_ attributed: NSAttributedString, width: CGFloat) -> CGSize {
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)
        var fitRange = CFRangeMake(0, 0)
        return CTFramesetterSuggestFrameSizeWithConstraints(
            framesetter,
            CFRangeMake(0, 0),
            nil,
            CGSize(width: width, height: .greatestFiniteMagnitude),
            &fitRange
        )
    }

    // MARK: - 2. Drawing a CoreText frame into a PDF page

    static func probeWritePaginatedPDF(text: String, to path: String) -> Bool {
        let pageSize = CGSize(width: 1280, height: 720)
        let margin: CGFloat = 60
        let textRect = CGRect(
            x: margin, y: margin,
            width: pageSize.width - margin * 2,
            height: pageSize.height - margin * 2
        )

        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 28)]
        )
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)
        let renderer = UIGraphicsPDFRenderer(bounds: CGRect(origin: .zero, size: pageSize))

        do {
            try renderer.writePDF(to: URL(fileURLWithPath: path)) { ctx in
                var location = 0
                let total = attributed.length
                while location < total {
                    ctx.beginPage()
                    let cg = ctx.cgContext

                    // CoreText draws bottom-up; flip into UIKit's coordinate space.
                    cg.textMatrix = .identity
                    cg.translateBy(x: 0, y: pageSize.height)
                    cg.scaleBy(x: 1.0, y: -1.0)

                    let path = CGPath(rect: textRect, transform: nil)
                    let frame = CTFramesetterCreateFrame(
                        framesetter,
                        CFRangeMake(location, 0),
                        path,
                        nil
                    )
                    CTFrameDraw(frame, cg)

                    let visible = CTFrameGetVisibleStringRange(frame)
                    if visible.length <= 0 { break }
                    location += visible.length
                }
            }
            return true
        } catch {
            return false
        }
    }

    // MARK: - 3. Per-line drawing, for a table layout the framesetter can't do

    static func probeDrawSingleLine(_ text: String, in ctx: CGContext, at point: CGPoint) {
        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 24)]
        )
        let line = CTLineCreateWithAttributedString(attributed)
        ctx.textPosition = point
        CTLineDraw(line, ctx)
    }

    static func probeLineWidth(_ text: String) -> Double {
        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 24)]
        )
        let line = CTLineCreateWithAttributedString(attributed)
        return CTLineGetTypographicBounds(line, nil, nil, nil)
    }

    // MARK: - 4. Korean font availability is a RUNTIME question, not a compile one

    static func probeKoreanFontNames() -> [String] {
        var found: [String] = []
        for family in UIFont.familyNames {
            for name in UIFont.fontNames(forFamilyName: family) {
                if name.contains("Gothic") || name.contains("Myungjo") || name.contains("Apple SD") {
                    found.append(name)
                }
            }
        }
        return found
    }

    // MARK: - 5. Logging, for the on-screen diagnostics pane

    static let log = Logger(subsystem: "com.sojubada.argo", category: "diagnostics")

    static func probeLogging() {
        log.info("probe: logger is available")
        os_log("probe: os_log is available")
    }

    // MARK: - 6. Temp file lifecycle, for the convert-then-delete flow

    static func probeTempLifecycle() -> Bool {
        let dir = (NSTemporaryDirectory() as NSString)
            .appendingPathComponent("argo_docpdf_\(UUID().uuidString)")
        let fm = FileManager.default
        guard (try? fm.createDirectory(atPath: dir, withIntermediateDirectories: true)) != nil else {
            return false
        }
        let file = (dir as NSString).appendingPathComponent("out.pdf")
        let ok = probeWritePaginatedPDF(text: "probe", to: file)
        let exists = fm.fileExists(atPath: file)
        try? fm.removeItem(atPath: dir)
        return ok && exists && !fm.fileExists(atPath: dir)
    }
}
```

---

## 2. 고칠 함수 — `extractParagraphs` (현재 상태)

`OfficeDocReaderView.swift` 385~431행입니다.

```swift
    private static func extractParagraphs(fromXML xml: String, paragraphTag: String, textTag: String) -> [String] {
        var result: [String] = []
        let openP = "<\(paragraphTag)"
        let closeP = "</\(paragraphTag)>"
        let openT = "<\(textTag)"
        let closeT = "</\(textTag)>"

        let pBlocks = xml.components(separatedBy: openP)
        for pBlock in pBlocks.dropFirst() {
            let content = pBlock.components(separatedBy: closeP).first ?? pBlock
            var paragraphText = ""
            let tBlocks = content.components(separatedBy: openT)
            for tBlock in tBlocks.dropFirst() {
                if let gtRange = tBlock.range(of: ">"),
                   let endRange = tBlock.range(of: closeT, range: gtRange.upperBound..<tBlock.endIndex) {
                    let raw = String(tBlock[gtRange.upperBound..<endRange.lowerBound])
                    paragraphText += decodeXMLEntities(raw)
                }
            }
            let trimmed = paragraphText.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                result.append(trimmed)
            }
        }
        return result
    }

    private static func stripAllXMLTags(_ xml: String) -> [String] {
        var text = ""
        var insideTag = false
        for ch in xml {
            if ch == "<" {
                insideTag = true
                text.append(" ")
            } else if ch == ">" {
                insideTag = false
            } else if !insideTag {
                text.append(ch)
            }
        }
        let decoded = decodeXMLEntities(text)
        return decoded
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

```

---

## 3. 화면 쪽에서 고칠 부분 (현재 상태)

### 3-1. 본문 (배경색 포함) — 41행

```swift
    var body: some View {
        ZStack {
            (isNightMode ? Color.black : Color(white: 0.12))
                .edgesIgnoringSafeArea(.all)

            VStack(spacing: 20) {
                headerBar

                if isLoading {
                    Spacer()
                    ProgressView("Reading document...")
                    Spacer()
                } else if let err = errorMessage {
                    Spacer()
                    Text(err)
                        .font(.headline)
                        .foregroundColor(.secondary)
                        .multilineTextAlignment(.center)
                    Spacer()
                } else {
                    if sections.count > 1 {
                        sectionTabsBar
                    }
                    documentContentList
                }
            }
            .padding(44)
        }
        .onAppear {
            loadDocumentAsync()
        }
        .onExitCommand {
            isPresented = false
        }
    }
```

### 3-2. 내용 목록 (줄번호·줄단위 버튼) — 128행

```swift
    private var documentContentList: some View {
        let currentLines = (selectedSectionIndex < sections.count) ? sections[selectedSectionIndex].lines : []
        return Group {
            if currentLines.isEmpty {
                Spacer()
                Text("No readable text found in this section.")
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                // 행당 포커스 버튼 하나 규칙 준수 (리모컨 상하 스와이프로 문단·행 단위 스크롤)
                List(currentLines.indices, id: \.self) { lineIdx in
                    Button(action: {}) {
                        HStack(alignment: .top, spacing: 16) {
                            Text("\(lineIdx + 1)")
                                .font(.caption)
                                .foregroundColor(.secondary)
                                .frame(width: 50, alignment: .trailing)

                            Text(currentLines[lineIdx])
                                .font(.body)
                                .foregroundColor(isNightMode ? Color(white: 0.92) : .primary)
                                .multilineTextAlignment(.leading)

                            Spacer()
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
        }
    }
```

### 3-3. 불러오기 — 170행

```swift
    private func loadDocumentAsync() {
        let path = fullPath
        DispatchQueue.global(qos: .userInitiated).async {
            let parsed = OfficeDocumentParser.parse(atPath: path)
            DispatchQueue.main.async {
                self.isLoading = false
                if parsed.isEmpty {
                    self.errorMessage = "Could not extract readable text from this file."
                } else {
                    self.sections = parsed
                    self.selectedSectionIndex = 0
                }
            }
        }
    }
```

---

## 4. `PDFReaderView` 를 띄우는 방법

변환한 PDF를 이 뷰에 넘기면 됩니다.

```swift
struct PDFReaderView: View {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    @StateObject private var model: PDFReaderModel
    @State private var showToolbar: Bool = false
    @State private var toolbarAutoHideTask: DispatchWorkItem? = nil

    @State private var passwordInput: String = ""
    @State private var passwordError: String = ""

    @FocusState private var canvasFocused: Bool

    init(filePath: String, fileName: String, isPresented: Binding<Bool>) {
        self.filePath = filePath
        self.fileName = fileName
        self._isPresented = isPresented
        self._model = StateObject(wrappedValue: PDFReaderModel(filePath: filePath, fileName: fileName))
    }

```

---

## 5. 설정 화면 — 진단(Lab) 진입점을 여기 맨 아래에 다십시오

`Spartan/ButtonActions/SettingsView.swift` 앞부분입니다 (전체 310행).

```swift
//
//  SettingsView.swift
//  Spartan
//
//  Created by RealKGB on 4/6/23.
//

import SwiftUI

struct SettingsView: View {
    @State private var showView: [Bool] = [Bool](repeating: false, count: 3)
    @State private var descriptiveTitlesPre = UserDefaults.settings.bool(forKey: "descriptiveTitles")
    @State private var descriptiveTimestampsPre = UserDefaults.settings.bool(forKey: "verboseTimestamps")
    @State private var autoCompletePre = UserDefaults.settings.bool(forKey: "autoComplete")
    @State private var logWindowFontSizePre = UserDefaults.settings.integer(forKey: "logWindowFontSize")
    @State private var sheikahFontApplyPre = UserDefaults.settings.bool(forKey: "sheikahFontApply")
    @State private var dateFormatPre: String = UserDefaults.settings.string(forKey: "dateFormat") ?? ""
    
    @State var test = 0.0

    var body: some View {
		ScrollView {
			Text(NSLocalizedString("SETTINGS", comment: "But choose carefully because you'll stay in the job you pick for the rest of your life."))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 60)
				}
				.font(.system(size: 60))
                .sheet(isPresented: $showView[0], content: {
                    CreditsView()
                })
			
			StepperTV(value: $logWindowFontSizePre, isHorizontal: true) {
				UserDefaults.settings.set(logWindowFontSizePre, forKey: "logWindowFontSize")
				UserDefaults.settings.synchronize()
			}
			.padding(5)
			Text(NSLocalizedString("SETTINGS_LOGFONTSIZE", comment: "The same job the rest of your life?"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
				.font(.system(size: 25))
				
			Text(" ")
				
			descriptiveThings
			Text(" ")
			autoCompleteFileExtensions
			Text(" ")
			sheikahFont
			Text(" ")
			
			GeometryReader { geometry in
				HStack {
					Spacer()
					TextField("SETTINGS_DATEFORMAT", text: $dateFormatPre)
						.frame(maxWidth: geometry.size.width * 0.8)
					Spacer()
				}
			}
			Text(" ")
				.font(.system(size: 15))
			Text(NSLocalizedString("SETTINGS_DATEFORMATDESC1", comment: "Halls Of Science 4"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
				.font(.system(size: 25))
			Text(NSLocalizedString("SETTINGS_DATEFORMATDESC2", comment: "Halls Of Science... 5?"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
```

