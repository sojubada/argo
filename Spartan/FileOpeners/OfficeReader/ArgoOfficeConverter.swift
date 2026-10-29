//
//  ArgoOfficeConverter.swift
//  Argo
//
//  오피스 문서를 기기 안에서 PDF 로 바꿉니다.
//
//  WebKit 안에서 JS 로 그리던 길과 완전히 다릅니다. 변환이 끝나면 남는 것은
//  PDF 하나뿐이고, 그건 이미 잘 도는 CoreGraphics 뷰어가 맡습니다 —
//  쪽 넘김도 두 쪽 보기도 맞춤도 공짜로 따라옵니다.
//

import UIKit

@available(tvOS 13.0, *)
enum ArgoOfficeConverter {

    enum Failure: Error {
        case unreadable
        case conversionFailed(Int32)
    }

    /// 변환기에 맡길 확장자.
    ///
    /// pptx 도 들어왔습니다 — 슬라이드 제목이 사라지던 것은 레이아웃이
    /// 마스터의 위치를 덮어쓰던 문제였고, 벤더링한 사본에서 고쳤습니다.
    ///
    /// xlsx 도 들어왔습니다. 빼 두었던 이유는 넓은 시트를 엑셀 인쇄하듯
    /// 열 묶음별로 쪼개서 화면으로 따라 읽을 수가 없었기 때문인데, 이제는
    /// 쪽을 시트 폭에 맞춰 늘리고 행만 넘깁니다.
    static let handled: Set<String> = ["docx", "pptx", "xlsx", "xls", "hwp", "hwpx"]

    /// 직전 변환이 사용자에게 알릴 것을 남겼으면 그 한 줄. 없으면 nil.
    ///
    /// 레이아웃이 요청대로 못 간 경우에만 채워집니다 — 넓은 시트를 쪽으로
    /// 나눴다든지. 조용히 다르게 그리면 문서가 깨진 것으로 보입니다.
    private(set) static var lastUserNotice: String?

    private static func readUserNotice() -> String? {
        var scratch = [UInt8](repeating: 0, count: 512)
        let n = scratch.withUnsafeMutableBufferPointer { raw -> Int in
            guard let base = raw.baseAddress else { return 0 }
            return argoconv_last_user_notice(base, raw.count)
        }
        guard n > 0, let text = String(bytes: scratch[0..<n], encoding: .utf8),
              !text.isEmpty else { return nil }
        return text
    }

    static func handles(_ path: String) -> Bool {
        handled.contains((path as NSString).pathExtension.lowercased())
    }

    // MARK: 폰트

    /// 예전 입구. 이제 글꼴은 문서마다 ArgoFontLibrary.prepare 가 고른다.
    /// 폰트 폴더만 미리 만들어 둔다.
    static func registerFonts() {
        ArgoFontLibrary.ensureFolder()
    }

    // MARK: 변환

    /// 문서를 PDF 로 바꾸고 그 파일 경로를 돌려줍니다.
    ///
    /// 바이트로 주고받습니다. tvOS 의 std 는 파일 메타데이터 쪽이 아직
    /// 비어 있어서, 읽고 쓰는 일은 이쪽에서 합니다.
    static func convertToPDF(at path: String) throws -> String {
        registerFonts()

        guard let input = FileManager.default.contents(atPath: path) else {
            throw Failure.unreadable
        }

        var out: UnsafeMutablePointer<UInt8>?
        var outLen = 0
        let started = Date()

        // 한글 문서는 다른 길로 갑니다. hwpers 가 쪽마다 SVG 로 그리고
        // 그걸 PDF 로 옮깁니다 — 끝에 나오는 것은 똑같이 PDF 하나입니다.
        let isHangul = ["hwp", "hwpx"].contains((path as NSString).pathExtension.lowercased())

        // 이 문서가 쓰는 글꼴을 폰트 폴더에서 찾아 넣는다. 없으면 닮은 것으로.
        let swaps = ArgoFontLibrary.prepare(for: input, consumer: isHangul ? .hangul : .office)

        let code = input.withUnsafeBytes { raw -> Int32 in
            guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return ARGOCONV_BAD_ARGS }
            return isHangul
                ? argoconv_hwp_to_pdf(base, input.count, &out, &outLen)
                : argoconv_to_pdf(base, input.count, &out, &outLen)
        }

        guard code == ARGOCONV_OK, let buffer = out, outLen > 0 else {
            // 왜 실패했는지 꺼내 둡니다. 패닉이면 패닉 메시지, 아니면
            // 변환기가 남긴 한 줄입니다 — 파일을 못 읽은 것인지 읽고도
            // 못 그린 것인지 로그만으로 갈려야 합니다.
            var scratch = [UInt8](repeating: 0, count: 1024)
            let n = scratch.withUnsafeMutableBufferPointer { raw -> Int in
                guard let base = raw.baseAddress else { return 0 }
                return code == ARGOCONV_PANICKED
                    ? argoconv_last_error(base, raw.count)
                    : argoconv_last_note(base, raw.count)
            }
            let why = n > 0
                ? " — " + (String(bytes: scratch[0..<n], encoding: .utf8) ?? "")
                : ""
            ArgoDiag.log("convert", "실패 code=\(code) \((path as NSString).lastPathComponent)\(why)")
            throw Failure.conversionFailed(code)
        }
        defer { argoconv_free(buffer, outLen) }

        // 바꿔 넣은 글꼴이 있으면 알린다. 조용히 바꾸면 문서가 원래 그렇게 생긴 줄 안다.
        var notices: [String] = []
        if let n = readUserNotice() { notices.append(n) }
        if !swaps.isEmpty {
            let shown = swaps.prefix(3).joined(separator: ", ") + (swaps.count > 3 ? LF(" 외 {0}개", swaps.count - 3) : "")
            notices.append(LF("없는 글꼴을 비슷한 글꼴로 그렸습니다: {0}", shown))
        }
        lastUserNotice = notices.isEmpty ? nil : notices.joined(separator: "  ·  ")

        let pdf = Data(bytes: buffer, count: outLen)
        let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
        let dest = NSTemporaryDirectory() + "/argo-\(name).pdf"
        try pdf.write(to: URL(fileURLWithPath: dest), options: .atomic)

        // 변환기가 무엇을 봤는지도 같이 적습니다. 표가 안 보일 때 못 읽은
        // 것인지 못 그린 것인지 이 한 줄로 갈립니다.
        var scratch = [UInt8](repeating: 0, count: 512)
        let noted = scratch.withUnsafeMutableBufferPointer { raw -> Int in
            guard let base = raw.baseAddress else { return 0 }
            return argoconv_last_note(base, raw.count)
        }
        let note = noted > 0 ? " — " + (String(bytes: scratch[0..<noted], encoding: .utf8) ?? "") : ""

        ArgoDiag.log("convert", "\((path as NSString).lastPathComponent) "
                               + LF("{0} → {1}바이트, ", input.count, outLen)
                               + String(format: L("%.2f초"), Date().timeIntervalSince(started))
                               + note)
        return dest
    }
}
