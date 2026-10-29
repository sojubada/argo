//
//  ArgoFontLibrary.swift
//  Argo
//
//  문서를 그릴 글꼴을 고른다.
//
//  사용자가 폰트 폴더(/var/mobile/Media/Argo/fonts)에 글꼴을 넣어 두면,
//  문서를 열 때마다 그 문서가 쓰는 글꼴 이름을 읽어 같은 것을 찾아 넣는다.
//  없으면 닮은 것(명조는 명조, 고딕은 고딕 …)을 골라 그 이름으로 넣는다.
//  그것도 없으면 앱에 든 Pretendard 와 기기의 Apple SD 산돌고딕으로 그린다.
//
//  - 글꼴을 통째로 올려 두지 않는다. 한 개에 10~20MB 라 수십 개면 메모리가
//    모자란다. 문서마다 필요한 것만 넣고 다음 문서 전에 비운다.
//  - 폴더를 처음 훑을 때 이름표만 읽어 색인을 만들고, 파일이 바뀌지 않았으면
//    다음부터는 색인을 그대로 쓴다.
//  - 하위 폴더도 훑는다. 글꼴 묶음을 폴더째 넣어도 된다.
//

import Foundation

struct ArgoFontFace: Codable {
    let path: String
    let index: Int
    let names: [String]
    let weight: Int
    let italic: Bool
    let mono: Bool
    let hangul: Bool
    /// PDF 에 심을 수 있는 TrueType 윤곽. 오피스 문서 쪽 변환기는 이것만 쓴다.
    let ttf: Bool

    var primaryName: String { names.first ?? (path as NSString).lastPathComponent }
    var isBold: Bool { weight >= 600 }
}

@available(tvOS 13.0, *)
enum ArgoFontLibrary {

    static let userDir = "/var/mobile/Media/Argo/fonts"
    private static let indexName = ".argo-font-index.json"

    /// 어떤 변환기에 넣을 글꼴인가. 오피스 쪽은 TrueType 만 심을 수 있다.
    enum Consumer { case office, hangul }

    // MARK: 색인

    private struct IndexFile: Codable {
        var version = 1
        var files: [String: Entry] = [:]
        struct Entry: Codable {
            var size: Int64
            var modified: Double
            var faces: [ArgoFontFace]
        }
    }

    private static var cached: [ArgoFontFace]?

    /// 폰트 폴더가 없으면 만든다. 무엇을 넣으면 되는지 적은 안내 파일도 둔다.
    static func ensureFolder() {
        let fm = FileManager.default
        if !fm.fileExists(atPath: userDir) {
            try? fm.createDirectory(atPath: userDir, withIntermediateDirectories: true)
        }
        let readme = (userDir as NSString).appendingPathComponent("README.txt")
        if !fm.fileExists(atPath: readme) {
            let text = """
            Argo 폰트 폴더 / Argo font folder

            문서를 그릴 글꼴(.ttf · .otf · .ttc)을 이 폴더에 넣으십시오. 하위 폴더도 됩니다.
            문서가 쓰는 글꼴과 같은 이름이 있으면 그것을, 없으면 닮은 글꼴을 골라 그립니다.
            엑셀·워드·파워포인트 문서는 .ttf (TrueType) 만 쓸 수 있습니다.

            Put .ttf / .otf / .ttc fonts here (subfolders are fine). Argo uses the font a
            document asks for when it is here, and a similar one when it is not.
            Office documents can only use .ttf (TrueType) fonts.
            """
            try? text.write(toFile: readme, atomically: true, encoding: .utf8)
        }
    }

    /// 쓸 수 있는 글꼴 얼굴 전부. 사용자 폴더 → 앱에 든 것 → 기기 것 순서다.
    static func allFaces() -> [ArgoFontFace] {
        if let cached { return cached }
        ensureFolder()
        var faces = indexedUserFaces()
        for name in ["Pretendard-Regular", "Pretendard-Bold"] {
            if let p = Bundle.main.path(forResource: name, ofType: "ttf") { faces += readFaces(p) }
        }
        for p in systemCandidates where FileManager.default.fileExists(atPath: p) {
            faces += readFaces(p)
        }
        cached = faces
        ArgoDiag.log("font", "글꼴 얼굴 \(faces.count)개 (사용자 폴더 \(faces.filter { $0.path.hasPrefix(userDir) }.count)개)")
        return faces
    }

    /// 폴더 내용이 바뀌었을 수 있을 때. 다음에 allFaces 를 부르면 다시 훑는다.
    static func invalidate() { cached = nil }

    private static let systemCandidates = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Core/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/CoreAddition/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/LanguageSupport/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Core/SFUI.ttf",
    ]

    private static func indexedUserFaces() -> [ArgoFontFace] {
        let fm = FileManager.default
        let indexPath = (userDir as NSString).appendingPathComponent(indexName)
        var index = (try? Data(contentsOf: URL(fileURLWithPath: indexPath)))
            .flatMap { try? JSONDecoder().decode(IndexFile.self, from: $0) } ?? IndexFile()
        var seen = Set<String>()
        var changed = false
        var out: [ArgoFontFace] = []

        let walker = fm.enumerator(atPath: userDir)
        while let rel = walker?.nextObject() as? String {
            let ext = (rel as NSString).pathExtension.lowercased()
            guard ["ttf", "otf", "ttc", "otc"].contains(ext) else { continue }
            let path = (userDir as NSString).appendingPathComponent(rel)
            guard let attrs = try? fm.attributesOfItem(atPath: path) else { continue }
            let size = (attrs[.size] as? NSNumber)?.int64Value ?? 0
            let mod = (attrs[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            seen.insert(path)
            if let e = index.files[path], e.size == size, e.modified == mod {
                out += e.faces
                continue
            }
            let faces = readFaces(path)
            index.files[path] = .init(size: size, modified: mod, faces: faces)
            changed = true
            out += faces
        }
        // 지운 파일은 색인에서도 뺀다.
        for path in index.files.keys where !seen.contains(path) {
            index.files.removeValue(forKey: path)
            changed = true
        }
        if changed, let data = try? JSONEncoder().encode(index) {
            try? data.write(to: URL(fileURLWithPath: indexPath), options: .atomic)
        }
        return out
    }

    /// 글꼴 파일 하나의 이름표를 읽는다. 파일을 메모리에 올리지 않고 비춰 본다.
    static func readFaces(_ path: String) -> [ArgoFontFace] {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .alwaysMapped) else { return [] }
        struct Raw: Decodable {
            let i: Int; let names: [String]; let weight: Int
            let italic: Bool; let mono: Bool; let hangul: Bool; let ttf: Bool
        }
        var cap = 64 * 1024
        for _ in 0..<2 {
            var buffer = [UInt8](repeating: 0, count: cap)
            let n = data.withUnsafeBytes { raw -> Int in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return buffer.withUnsafeMutableBufferPointer { argoconv_font_faces(base, data.count, $0.baseAddress, cap) }
            }
            if n > cap { cap = n; continue }
            guard n > 0, let list = try? JSONDecoder().decode([Raw].self, from: Data(buffer[0..<n])) else { return [] }
            return list.map {
                ArgoFontFace(path: path, index: $0.i, names: $0.names, weight: $0.weight,
                             italic: $0.italic, mono: $0.mono, hangul: $0.hangul, ttf: $0.ttf)
            }
        }
        return []
    }

    // MARK: 문서에 맞추기

    /// 문서 하나를 그리기 전에 부른다. 등록을 비우고, 기본 글꼴과 그 문서가 원하는
    /// 글꼴(또는 닮은 것)을 넣는다. 바꿔 넣은 것들을 사람이 읽을 줄로 돌려준다.
    @discardableResult
    static func prepare(for document: Data, consumer: Consumer) -> [String] {
        argoconv_clear_fonts()
        let faces = allFaces()

        // 기본: 모든 글자를 그릴 수 있게 받쳐 주는 글꼴. 이름은 예전과 같다.
        var registered = Set<String>()
        func register(_ face: ArgoFontFace, as alias: String) {
            let key = alias.lowercased()
            guard !registered.contains(key),
                  let data = try? Data(contentsOf: URL(fileURLWithPath: face.path), options: .alwaysMapped)
            else { return }
            registered.insert(key)
            _ = data.withUnsafeBytes { raw -> Int32 in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return -1 }
                return argoconv_register_font(alias, base, data.count)
            }
        }
        for face in faces where !face.path.hasPrefix(userDir) && face.index == 0 {
            register(face, as: ((face.path as NSString).lastPathComponent as NSString).deletingPathExtension.lowercased())
        }

        var report: [String] = []
        for wanted in documentFonts(document) {
            guard let pick = resolve(wanted, in: faces, consumer: consumer) else { continue }
            register(pick.regular, as: wanted)
            if let bold = pick.bold { register(bold, as: wanted + " bold") }
            _ = argoconv_font_alias(wanted, pick.regular.primaryName)
            if !pick.exact {
                report.append("\(wanted) → \(pick.regular.primaryName)")
            }
            ArgoDiag.log("font", "\(wanted) → \(pick.regular.primaryName)\(pick.bold.map { " / 굵게 " + $0.primaryName } ?? "")\(pick.exact ? "" : " (닮은 글꼴)")")
        }
        return report
    }

    /// 문서가 쓰는 글꼴 이름들.
    static func documentFonts(_ document: Data) -> [String] {
        var cap = 16 * 1024
        for _ in 0..<2 {
            var buffer = [UInt8](repeating: 0, count: cap)
            let n = document.withUnsafeBytes { raw -> Int in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return buffer.withUnsafeMutableBufferPointer { argoconv_doc_fonts(base, document.count, $0.baseAddress, cap) }
            }
            if n > cap { cap = n; continue }
            guard n > 0 else { return [] }
            return (try? JSONDecoder().decode([String].self, from: Data(buffer[0..<n]))) ?? []
        }
        return []
    }

    struct Pick {
        let regular: ArgoFontFace
        let bold: ArgoFontFace?
        /// 같은 글꼴을 찾았는가. 아니면 닮은 것으로 바꾼 것이다.
        let exact: Bool
    }

    /// 이름으로 찾고, 없으면 닮은 것을 고른다.
    static func resolve(_ wanted: String, in faces: [ArgoFontFace], consumer: Consumer) -> Pick? {
        let usable = faces.filter { consumer == .hangul || $0.ttf }
        guard !usable.isEmpty else { return nil }
        let keys = Set([normalize(wanted)] + aliases(of: wanted))

        // 1. 같은 이름 (이름표의 어느 말로든)
        let same = usable.filter { face in face.names.contains { keys.contains(normalize(stripStyle($0))) || keys.contains(normalize($0)) } }
        if !same.isEmpty {
            return Pick(regular: bestRegular(same), bold: bestBold(same), exact: true)
        }

        // 2. 닮은 갈래 (명조·고딕·손글씨·고정폭·제목용)
        let want = category(of: [wanted])
        let userFaces = usable.filter { $0.path.hasPrefix(userDir) }
        let pool = userFaces.isEmpty ? usable : userFaces
        var candidates = pool.filter { category(of: $0.names) == want }
        if candidates.isEmpty && want == .display { candidates = pool.filter { category(of: $0.names) == .sans } }
        if candidates.isEmpty && want != .sans { candidates = pool.filter { category(of: $0.names) == .sans } }
        if candidates.isEmpty { candidates = pool }
        // 한글을 그릴 수 있는 것을 먼저 둔다. 한글 문서가 대부분이고, 영문 글꼴로
        // 바꾸면 한글 부분이 다른 글꼴로 갈라져 보인다.
        let withHangul = candidates.filter { $0.hangul }
        if !withHangul.isEmpty { candidates = withHangul }
        // 같은 가족끼리 묶어 첫 가족에서 보통·굵게를 고른다.
        guard let first = candidates.first(where: { !$0.italic && !$0.isBold }) ?? candidates.first else { return nil }
        let family = candidates.filter { $0.names.first.map(normalize) == first.names.first.map(normalize) }
        let familyOrFirst = family.isEmpty ? [first] : family
        let regular = bestRegular(familyOrFirst)
        let bold = want == .display ? nil : bestBold(familyOrFirst)
        return Pick(regular: want == .display ? (bestBold(familyOrFirst) ?? regular) : regular,
                    bold: bold, exact: false)
    }

    private static func bestRegular(_ faces: [ArgoFontFace]) -> ArgoFontFace {
        faces.min { score($0, weight: 400) < score($1, weight: 400) } ?? faces[0]
    }

    private static func bestBold(_ faces: [ArgoFontFace]) -> ArgoFontFace? {
        let bold = faces.filter { $0.isBold && !$0.italic }
        return bold.min { score($0, weight: 700) < score($1, weight: 700) }
    }

    private static func score(_ f: ArgoFontFace, weight: Int) -> Int {
        abs(f.weight - weight) + (f.italic ? 500 : 0) + (f.index == 0 ? 0 : 50)
    }

    // MARK: 이름 다루기

    static func normalize(_ s: String) -> String {
        String(s.lowercased().unicodeScalars.filter { !CharacterSet(charactersIn: " -_·.").contains($0) })
    }

    /// "나눔고딕 Bold" → "나눔고딕". 굵기 말을 떼고 가족 이름만 본다.
    private static func stripStyle(_ s: String) -> String {
        var t = s
        for w in [" Regular", " Bold", " Light", " Medium", " Thin", " ExtraBold", " Black", " Heavy",
                  " SemiBold", " ExtraLight", " Italic", " Oblique"] {
            if t.hasSuffix(w) { t = String(t.dropLast(w.count)) }
        }
        return t
    }

    /// 한국어 이름과 영어 이름이 다른 글꼴. 이름표에 둘 다 있는 경우가 많지만,
    /// 한쪽만 적어 둔 파일도 있어 짝을 알아 둔다.
    private static let pairs: [[String]] = [
        ["맑은고딕", "malgungothic"], ["바탕", "batang"], ["바탕체", "batangche"],
        ["굴림", "gulim"], ["굴림체", "gulimche"], ["돋움", "dotum"], ["돋움체", "dotumche"],
        ["궁서", "gungsuh"], ["궁서체", "gungsuhche"], ["새굴림", "newgulim"],
        ["함초롬바탕", "hcrbatang"], ["함초롬돋움", "hcrdotum"],
        ["나눔고딕", "nanumgothic"], ["나눔명조", "nanummyeongjo"], ["나눔바른고딕", "nanumbarungothic"],
        ["나눔스퀘어", "nanumsquare"], ["본고딕", "sourcehansanskr", "notosanskr", "notosanscjkkr"],
        ["본명조", "sourcehanserifkr", "notoserifkr", "notoserifcjkkr"],
        ["애플산돌고딕네오", "applesdgothicneo"], ["한컴바탕", "hancombatang"], ["한컴돋움", "hancomdotum"],
    ]

    private static func aliases(of name: String) -> [String] {
        let n = normalize(name)
        return pairs.first { $0.contains(n) } ?? []
    }

    enum Category { case serif, sans, brush, mono, display }

    /// 이름으로 갈래를 짐작한다. 맞는 말이 없으면 고딕(sans)으로 본다.
    static func category(of names: [String]) -> Category {
        let all = names.map(normalize).joined(separator: " ")
        func has(_ words: [String]) -> Bool { words.contains { all.contains($0) } }
        if has(["궁서", "gungsuh", "붓", "손글씨", "필기", "흘림", "펜글씨", "brush", "script", "handwriting", "pen", "calligraph", "붓글씨"]) { return .brush }
        if has(["mono", "coding", "courier", "consolas", "d2coding", "고정폭", "바탕체", "굴림체", "돋움체", "batangche", "gulimche", "dotumche", "menlo"]) { return .mono }
        if has(["헤드라인", "headline", "견고딕", "태고딕", "impact", "display", "poster"]) { return .display }
        if has(["명조", "바탕", "myeongjo", "myungjo", "batang", "serif", "times", "garamond", "georgia", "cambria", "mincho", "song", "신명", "순명", "궁체"]) {
            // "sans serif" 는 고딕이다.
            if all.contains("sans") { return .sans }
            return .serif
        }
        return .sans
    }
}
