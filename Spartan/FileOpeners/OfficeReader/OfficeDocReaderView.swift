//
//  OfficeDocReaderView.swift
//  Argo
//
//  Simple Document Reader for Word (.docx/.doc), Excel (.xlsx/.xls),
//  PowerPoint (.pptx/.ppt), and Hancom Office (.hwpx/.hwp).
//  Uses the existing Zip framework for OOXML/OWPML containers and
//  a binary string stream extractor fallback for legacy OLE documents.
//

import SwiftUI
import Foundation
import Zip

struct OfficeDocSection: Identifiable {
    let id = UUID()
    let title: String
    let lines: [String]
}

struct OfficeDocReaderView: View {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    @State private var sections: [OfficeDocSection] = []
    @State private var selectedSectionIndex: Int = 0
    @State private var isLoading: Bool = true
    @State private var isNightMode: Bool = false
    @State private var errorMessage: String? = nil

    private var fullPath: String {
        if filePath.hasSuffix("/") {
            return filePath + fileName
        } else if filePath.isEmpty {
            return fileName
        }
        return filePath + "/" + fileName
    }

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

    private var headerBar: some View {
        HStack(spacing: 20) {
            Image(systemName: iconForFile(fileName))
                .font(.title2)
            Text(fileName)
                .font(.title3)
                .lineLimit(1)

            Spacer()

            Button(action: {
                isNightMode.toggle()
            }) {
                HStack(spacing: 8) {
                    Image(systemName: isNightMode ? "moon.fill" : "sun.max.fill")
                    Text(isNightMode ? "Night" : "Day")
                }
            }

            Button(action: {
                isPresented = false
            }) {
                HStack(spacing: 8) {
                    Image(systemName: "xmark")
                    Text("Close")
                }
            }
        }
    }

    private var sectionTabsBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                ForEach(sections.indices, id: \.self) { idx in
                    Button(action: {
                        selectedSectionIndex = idx
                    }) {
                        HStack(spacing: 6) {
                            if selectedSectionIndex == idx {
                                Image(systemName: "checkmark.circle.fill")
                            }
                            Text(sections[idx].title)
                                .lineLimit(1)
                        }
                    }
                }
            }
            .padding(.vertical, 6)
        }
    }

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

    private func iconForFile(_ name: String) -> String {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "xls", "xlsx": return "tablecells"
        case "ppt", "pptx": return "rectangle.on.rectangle"
        case "hwp", "hwpx": return "doc.text.fill"
        default: return "doc.richtext"
        }
    }

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
}

// MARK: - Office / Hancom Document Parser

enum OfficeDocumentParser {

    static func parse(atPath path: String) -> [OfficeDocSection] {
        let ext = (path as NSString).pathExtension.lowercased()
        let tempDir = (NSTemporaryDirectory() as NSString).appendingPathComponent("argo_office_\(UUID().uuidString)")

        defer {
            try? fileManager.removeItem(atPath: tempDir)
        }

        // 1. ZIP 기반 문서 (.docx, .xlsx, .pptx, .hwpx)
        if ["docx", "xlsx", "pptx", "hwpx"].contains(ext) || hasZipMagicBytes(atPath: path) {
            if unzipContainer(atPath: path, toDir: tempDir) {
                switch ext {
                case "docx":
                    let res = parseDocx(in: tempDir)
                    if !res.isEmpty { return res }
                case "xlsx":
                    let res = parseXlsx(in: tempDir)
                    if !res.isEmpty { return res }
                case "pptx":
                    let res = parsePptx(in: tempDir)
                    if !res.isEmpty { return res }
                case "hwpx":
                    let res = parseHwpx(in: tempDir)
                    if !res.isEmpty { return res }
                default:
                    break
                }
                // 확장자가 다르더라도 내부 XML을 스캔해 추출
                let fallbackXML = parseAllXMLFiles(in: tempDir)
                if !fallbackXML.isEmpty { return fallbackXML }
            }
        }

        // 2. 구형 바이너리 문서 (.doc, .xls, .ppt, .hwp) -> UTF-16LE 및 ASCII 텍스트 스트림 추출
        return extractBinaryTextStreams(atPath: path)
    }

    private static func hasZipMagicBytes(atPath path: String) -> Bool {
        guard let handle = FileHandle(forReadingAtPath: path) else { return false }
        defer { try? handle.close() }
        let data = handle.readData(ofLength: 4)
        return data.count >= 2 && data[0] == 0x50 && data[1] == 0x4B
    }

    private static func unzipContainer(atPath path: String, toDir destDir: String) -> Bool {
        try? fileManager.createDirectory(atPath: destDir, withIntermediateDirectories: true, attributes: nil)
        let srcURL = URL(fileURLWithPath: path)
        let dstURL = URL(fileURLWithPath: destDir)

        // Zip 프레임워크는 알 수 없는 확장자(.hwpx 등)를 거부할 수 있으므로 임시 .zip 심볼릭 링크/사본 활용
        let tempZipPath = (NSTemporaryDirectory() as NSString).appendingPathComponent("argo_tmp_\(UUID().uuidString).zip")
        defer { try? fileManager.removeItem(atPath: tempZipPath) }

        do {
            try fileManager.copyItem(atPath: path, toPath: tempZipPath)
            try Zip.unzipFile(URL(fileURLWithPath: tempZipPath), destination: dstURL, overwrite: true, password: nil)
            return true
        } catch {
            do {
                try Zip.unzipFile(srcURL, destination: dstURL, overwrite: true, password: nil)
                return true
            } catch {
                return false
            }
        }
    }

    // MARK: - DOCX

    private static func parseDocx(in dir: String) -> [OfficeDocSection] {
        let docXML = (dir as NSString).appendingPathComponent("word/document.xml")
        guard let xml = try? String(contentsOfFile: docXML, encoding: .utf8) else { return [] }
        let lines = extractParagraphs(fromXML: xml, paragraphTag: "w:p", textTag: "w:t")
        guard !lines.isEmpty else { return [] }
        return [OfficeDocSection(title: "Document", lines: lines)]
    }

    // MARK: - XLSX

    private static func parseXlsx(in dir: String) -> [OfficeDocSection] {
        let sharedStringsPath = (dir as NSString).appendingPathComponent("xl/sharedStrings.xml")
        var sharedStrings: [String] = []
        if let ssXML = try? String(contentsOfFile: sharedStringsPath, encoding: .utf8) {
            sharedStrings = extractParagraphs(fromXML: ssXML, paragraphTag: "si", textTag: "t")
        }

        let sheetsDir = (dir as NSString).appendingPathComponent("xl/worksheets")
        guard let files = try? fileManager.contentsOfDirectory(atPath: sheetsDir) else { return [] }
        let sheetFiles = files.filter { $0.hasSuffix(".xml") }.sorted()

        var sections: [OfficeDocSection] = []
        for (idx, file) in sheetFiles.enumerated() {
            let full = (sheetsDir as NSString).appendingPathComponent(file)
            guard let xml = try? String(contentsOfFile: full, encoding: .utf8) else { continue }
            let rows = parseXlsxSheetRows(xml: xml, sharedStrings: sharedStrings)
            if !rows.isEmpty {
                sections.append(OfficeDocSection(title: "Sheet \(idx + 1)", lines: rows))
            }
        }
        return sections
    }

    private static func parseXlsxSheetRows(xml: String, sharedStrings: [String]) -> [String] {
        var rows: [String] = []
        let rowBlocks = xml.components(separatedBy: "<row")
        for block in rowBlocks.dropFirst() {
            guard let endRow = block.range(of: "</row>") else { continue }
            let rowContent = String(block[..<endRow.lowerBound])
            let cellBlocks = rowContent.components(separatedBy: "<c")
            var cellTexts: [String] = []

            for cell in cellBlocks.dropFirst() {
                let isShared = cell.contains("t=\"s\"")
                if let vStart = cell.range(of: "<v>"),
                   let vEnd = cell.range(of: "</v>", range: vStart.upperBound..<cell.endIndex) {
                    let rawVal = decodeXMLEntities(String(cell[vStart.upperBound..<vEnd.lowerBound]))
                    if isShared, let sIdx = Int(rawVal), sIdx >= 0, sIdx < sharedStrings.count {
                        cellTexts.append(sharedStrings[sIdx])
                    } else if !rawVal.isEmpty {
                        cellTexts.append(rawVal)
                    }
                } else if let tStart = cell.range(of: "<t"),
                          let gt = cell.range(of: ">", range: tStart.upperBound..<cell.endIndex),
                          let tEnd = cell.range(of: "</t>", range: gt.upperBound..<cell.endIndex) {
                    let inlineStr = decodeXMLEntities(String(cell[gt.upperBound..<tEnd.lowerBound]))
                    if !inlineStr.isEmpty {
                        cellTexts.append(inlineStr)
                    }
                }
            }
            if !cellTexts.isEmpty {
                rows.append(cellTexts.joined(separator: "   |   "))
            }
        }
        return rows
    }

    // MARK: - PPTX

    private static func parsePptx(in dir: String) -> [OfficeDocSection] {
        let slidesDir = (dir as NSString).appendingPathComponent("ppt/slides")
        guard let files = try? fileManager.contentsOfDirectory(atPath: slidesDir) else { return [] }
        let slideFiles = files.filter { $0.hasPrefix("slide") && $0.hasSuffix(".xml") }.sorted {
            extractTrailingNumber($0) < extractTrailingNumber($1)
        }

        var sections: [OfficeDocSection] = []
        for (idx, file) in slideFiles.enumerated() {
            let full = (slidesDir as NSString).appendingPathComponent(file)
            guard let xml = try? String(contentsOfFile: full, encoding: .utf8) else { continue }
            let lines = extractParagraphs(fromXML: xml, paragraphTag: "a:p", textTag: "a:t")
            if !lines.isEmpty {
                sections.append(OfficeDocSection(title: "Slide \(idx + 1)", lines: lines))
            }
        }
        return sections
    }

    // MARK: - HWPX (Hancom OWPML)

    private static func parseHwpx(in dir: String) -> [OfficeDocSection] {
        let contentsDir = (dir as NSString).appendingPathComponent("Contents")
        guard let files = try? fileManager.contentsOfDirectory(atPath: contentsDir) else { return [] }
        let sectionFiles = files.filter { $0.hasPrefix("section") && $0.hasSuffix(".xml") }.sorted()

        var sections: [OfficeDocSection] = []
        for (idx, file) in sectionFiles.enumerated() {
            let full = (contentsDir as NSString).appendingPathComponent(file)
            guard let xml = try? String(contentsOfFile: full, encoding: .utf8) else { continue }
            let lines = extractParagraphs(fromXML: xml, paragraphTag: "hp:p", textTag: "hp:t")
            if !lines.isEmpty {
                sections.append(OfficeDocSection(title: "Section \(idx + 1)", lines: lines))
            }
        }
        return sections
    }

    private static func parseAllXMLFiles(in dir: String) -> [OfficeDocSection] {
        guard let enumerator = fileManager.enumerator(atPath: dir) else { return [] }
        var lines: [String] = []
        for case let sub as String in enumerator {
            guard sub.hasSuffix(".xml") else { continue }
            let full = (dir as NSString).appendingPathComponent(sub)
            if let xml = try? String(contentsOfFile: full, encoding: .utf8) {
                let stripped = stripAllXMLTags(xml)
                lines.append(contentsOf: stripped)
            }
        }
        guard !lines.isEmpty else { return [] }
        return [OfficeDocSection(title: "Content", lines: lines)]
    }

    // MARK: - XML Helpers

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

    private static func decodeXMLEntities(_ input: String) -> String {
        return input
            .replacingOccurrences(of: "&amp;", with: "&")
            .replacingOccurrences(of: "&lt;", with: "<")
            .replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&apos;", with: "'")
    }

    private static func extractTrailingNumber(_ name: String) -> Int {
        let digits = name.filter { $0.isNumber }
        return Int(digits) ?? 0
    }

    // MARK: - Legacy Binary Stream Extractor (.doc, .xls, .ppt, .hwp)

    private static func extractBinaryTextStreams(atPath path: String) -> [OfficeDocSection] {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .mappedIfSafe) else {
            return []
        }

        var lines: [String] = []

        // 1. UTF-16LE 연속 문자열 스캔 (한글 가~힣 및 ASCII 인쇄 가능 문자)
        let count = data.count
        var currentUTF16: [UInt16] = []

        data.withUnsafeBytes { rawBuffer in
            guard let base = rawBuffer.bindMemory(to: UInt8.self).baseAddress else { return }
            var i = 0
            while i + 1 < count {
                let u = UInt16(base[i]) | (UInt16(base[i + 1]) << 8)
                let isPrintableASCII = (u >= 0x0020 && u <= 0x007E)
                let isHangulSyllable = (u >= 0xAC00 && u <= 0xD7A3)
                let isHangulJamo = (u >= 0x3131 && u <= 0x318E)
                let isNewlineOrTab = (u == 0x000A || u == 0x000D || u == 0x0009)

                if isPrintableASCII || isHangulSyllable || isHangulJamo || isNewlineOrTab {
                    currentUTF16.append(u)
                } else {
                    if currentUTF16.count >= 5 {
                        let s = String(utf16CodeUnits: currentUTF16, count: currentUTF16.count)
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                        if s.count >= 4 && !looksLikeBinaryNoise(s) {
                            lines.append(s)
                        }
                    }
                    currentUTF16.removeAll(keepingCapacity: true)
                }
                i += 2
            }
        }

        if lines.isEmpty {
            // 2. UTF-8 / ASCII 연속 문자열 폴백
            var asciiBytes: [UInt8] = []
            for byte in data {
                if (byte >= 32 && byte <= 126) || byte == 10 || byte == 13 {
                    asciiBytes.append(byte)
                } else {
                    if asciiBytes.count >= 8,
                       let s = String(bytes: asciiBytes, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
                       !s.isEmpty && !looksLikeBinaryNoise(s) {
                        lines.append(s)
                    }
                    asciiBytes.removeAll(keepingCapacity: true)
                }
            }
        }

        guard !lines.isEmpty else { return [] }
        return [OfficeDocSection(title: "Extracted Text (Legacy Format)", lines: Array(lines.prefix(2000)))]
    }

    private static func looksLikeBinaryNoise(_ text: String) -> Bool {
        let ignoredPrefixes = ["Root Entry", "Workbook", "WordDocument", "PowerPoint Document", "FileHeader", "BodyText", "Section0", "SummaryInformation", "DocumentSummaryInformation"]
        for p in ignoredPrefixes {
            if text == p { return true }
        }
        return false
    }
}