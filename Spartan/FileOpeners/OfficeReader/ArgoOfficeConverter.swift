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
    /// xlsx 는 당분간 뺍니다. 변환 자체는 되지만 엑셀의 인쇄 방식 그대로
    /// 넓은 시트를 열 묶음별로 쪼개서, 화면으로 따라 읽기가 어렵습니다.
    static let handled: Set<String> = ["docx", "pptx", "hwp", "hwpx"]

    static func handles(_ path: String) -> Bool {
        handled.contains((path as NSString).pathExtension.lowercased())
    }

    // MARK: 폰트

    private static var fontsReady = false

    /// 한글이 빈칸으로 나오지 않게 폰트를 먹입니다.
    ///
    /// tvOS 에는 fontconfig 가 없고, 변환기가 들고 있는 시스템 폰트 탐색도
    /// 여기서는 돌지 않습니다. 대신 기기에 있는 폰트를 우리가 읽어 넘깁니다.
    /// 사용자가 따로 넣어 둔 폰트도 같은 길로 들어옵니다.
    static func registerFonts() {
        guard !fontsReady else { return }
        fontsReady = true

        var candidates = [
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Core/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/CoreAddition/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Core/SFUI.ttf",
            "/System/Library/Fonts/LanguageSupport/AppleSDGothicNeo.ttc",
        ]

        // 사용자가 직접 넣는 자리. 기기에 없는 글꼴이 필요할 때 쓸 수 있게
        // 둡니다 — 파일 탐색기로 넣고 바로 쓰면 됩니다.
        let userDir = "/var/mobile/Media/Argo/fonts"
        if let found = try? FileManager.default.contentsOfDirectory(atPath: userDir) {
            candidates += found
                .filter { ["ttf", "ttc", "otf"].contains(($0 as NSString).pathExtension.lowercased()) }
                .map { userDir + "/" + $0 }
        }

        var loaded = 0
        for path in candidates {
            guard FileManager.default.fileExists(atPath: path),
                  let data = FileManager.default.contents(atPath: path) else { continue }
            let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
            let ok = data.withUnsafeBytes { raw -> Int32 in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return -1 }
                return argoconv_register_font(name, base, data.count)
            }
            if ok == ARGOCONV_OK { loaded += 1 }
            ArgoDiag.log("font", "\(name) \(data.count)바이트 → \(ok == ARGOCONV_OK ? "등록" : "실패")")
        }
        ArgoDiag.log("font", "등록한 폰트 \(loaded)개")
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
                               + "\(input.count) → \(outLen)바이트, "
                               + String(format: "%.2f초", Date().timeIntervalSince(started))
                               + note)
        return dest
    }
}
