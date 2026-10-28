//
//  ArgoTextTools.swift
//  Argo
//
//  글 파일을 다루는 공용 부품. 읽기 화면과 편집기가 같이 쓴다.
//
//  - ArgoTextKind:   확장자로 언어를 정한다. 주석 기호, 들여쓰기 단위도 여기서.
//  - ArgoTextCodec:  인코딩·줄바꿈·BOM 을 알아 두었다가 저장할 때 그대로 되돌린다.
//                    읽을 때 UTF-8 로 바꿔 저장해 버리면 CP949 로 된 한글 파일이
//                    다른 프로그램에서 깨진다.
//  - ArgoSyntax:     코드 색칠.
//  - ArgoMarkdown:   .md 를 읽기 좋게 그린다.
//

import UIKit

// MARK: - 언어

enum ArgoLanguage: String {
    case plain, swift, cFamily, javascript, python, shell, ruby, go, rust, java
    case html, xml, css, json, yaml, ini, markdown, log, sql, lua, makefile, csv
}

enum ArgoTextKind {

    /// 확장자 → 언어. 확장자가 없는 이름 몇 개도 본다(Makefile 등).
    static func language(for path: String) -> ArgoLanguage {
        let name = (path as NSString).lastPathComponent.lowercased()
        switch name {
        case "makefile", "gnumakefile": return .makefile
        case "dockerfile", ".bashrc", ".zshrc", ".profile", ".bash_profile": return .shell
        case ".gitignore", ".gitattributes", ".editorconfig": return .ini
        default: break
        }
        switch (name as NSString).pathExtension {
        case "swift": return .swift
        case "c", "h", "m", "mm", "cc", "cpp", "cxx", "hpp", "hh", "metal", "cs": return .cFamily
        case "js", "mjs", "cjs", "jsx", "ts", "tsx": return .javascript
        case "py", "pyw": return .python
        case "sh", "bash", "zsh", "command": return .shell
        case "rb": return .ruby
        case "go": return .go
        case "rs": return .rust
        case "java", "kt", "kts", "gradle", "scala", "dart": return .java
        case "html", "htm", "xhtml", "vue", "svelte": return .html
        case "xml", "plist", "svg", "storyboard", "xib", "entitlements", "strings", "xcscheme": return .xml
        case "css", "scss", "less": return .css
        case "json", "jsonc", "geojson", "ipynb": return .json
        case "yml", "yaml": return .yaml
        case "ini", "conf", "cfg", "toml", "properties", "env", "pbxproj": return .ini
        case "md", "markdown", "mdown", "mkd": return .markdown
        case "log", "out", "crash", "ips": return .log
        case "sql": return .sql
        case "lua": return .lua
        case "mk": return .makefile
        case "csv", "tsv": return .csv
        default: return .plain
        }
    }

    /// 확장자만 보고 글 파일로 칠 것들. 내용 검사보다 먼저 본다 —
    /// 실행 권한이 붙은 .sh 가 '실행 파일' 로, CP949 로 된 .txt 가 '모르는 파일'
    /// 로 분류되던 것을 막는다.
    static let textExtensions: Set<String> = [
        "txt", "text", "md", "markdown", "mdown", "mkd", "rst", "log", "out", "csv", "tsv",
        "json", "jsonc", "yml", "yaml", "toml", "ini", "conf", "cfg", "properties", "env",
        "swift", "c", "h", "m", "mm", "cc", "cpp", "cxx", "hpp", "hh", "metal", "cs",
        "js", "mjs", "cjs", "jsx", "tsx", "py", "pyw", "sh", "bash", "zsh", "command",
        "rb", "go", "rs", "java", "kt", "kts", "gradle", "scala", "dart", "lua", "sql",
        "css", "scss", "less", "xml", "strings", "entitlements", "storyboard", "xib",
        "pbxproj", "xcscheme", "mk", "srt", "vtt", "ass", "nfo", "diff", "patch", "tex",
        "gitignore", "editorconfig", "vue", "svelte"
    ]

    /// 줄 주석 기호. 없으면 블록 주석을 앞뒤로 두른다.
    static func lineComment(_ lang: ArgoLanguage) -> String? {
        switch lang {
        case .swift, .cFamily, .javascript, .go, .rust, .java: return "//"
        case .python, .shell, .ruby, .yaml, .makefile: return "#"
        case .ini: return "#"
        case .sql, .lua: return "--"
        default: return nil
        }
    }

    static func blockComment(_ lang: ArgoLanguage) -> (String, String)? {
        switch lang {
        case .html, .xml, .markdown: return ("<!--", "-->")
        case .css: return ("/*", "*/")
        default: return nil
        }
    }

    /// 들여쓰기 단위. 파일 안에서 쓰는 것을 먼저 따르고, 없으면 언어 관례.
    static func indentUnit(for text: String, language: ArgoLanguage) -> String {
        if language == .makefile || language == .go { return "\t" }
        var tabs = 0, spaced = 0
        var widths: [Int: Int] = [:]
        var lines = 0
        text.enumerateLines { line, stop in
            lines += 1
            if lines > 2000 { stop = true; return }
            if line.hasPrefix("\t") { tabs += 1; return }
            let n = line.prefix(while: { $0 == " " }).count
            if n > 0 && n < line.count {
                spaced += 1
                widths[n, default: 0] += 1
            }
        }
        if tabs > spaced && tabs > 2 { return "\t" }
        if spaced > 2 {
            // 2 칸 들여쓴 줄이 꽤 있으면 2, 아니면 4.
            let twos = widths.filter { $0.key % 4 == 2 }.map { $0.value }.reduce(0, +)
            return twos * 5 >= spaced ? "  " : "    "
        }
        switch language {
        case .javascript, .html, .xml, .css, .json, .yaml, .ruby, .markdown, .lua: return "  "
        default: return "    "
        }
    }

    /// 여는 괄호 뒤에서 줄을 바꾸면 한 단계 더 들여쓴다.
    static func opensBlock(_ trimmedLineBeforeCursor: String, language: ArgoLanguage) -> Bool {
        guard let last = trimmedLineBeforeCursor.last else { return false }
        if "{[(".contains(last) { return true }
        if last == ":" && (language == .python || language == .yaml) { return true }
        return false
    }
}

// MARK: - 인코딩

struct ArgoTextCodec {
    var encoding: String.Encoding = .utf8
    /// 원본이 CRLF 였으면 저장할 때 되돌린다. 편집 중에는 늘 \n 이다.
    var crlf = false
    var bom = false

    var label: String {
        var s: String
        switch encoding {
        case .utf8: s = "UTF-8"
        case .utf16, .utf16LittleEndian, .utf16BigEndian: s = "UTF-16"
        case ArgoTextCodec.cp949: s = "CP949"
        case .shiftJIS: s = "Shift-JIS"
        case .isoLatin1: s = "Latin-1"
        case .windowsCP1252: s = "CP1252"
        case .ascii: s = "ASCII"
        default: s = "인코딩 \(encoding.rawValue)"
        }
        if bom { s += " BOM" }
        if crlf { s += " · CRLF" }
        return s
    }

    /// 한국어 Windows 기본 인코딩.
    static let cp949 = String.Encoding(rawValue: CFStringConvertEncodingToNSStringEncoding(
        CFStringEncoding(CFStringEncodings.dosKorean.rawValue)))

    /// 데이터를 글로 푼다. 못 풀면 nil.
    static func decode(_ data: Data) -> (String, ArgoTextCodec)? {
        var codec = ArgoTextCodec()
        var body = data

        if data.starts(with: [0xEF, 0xBB, 0xBF]) {
            codec.bom = true
            body = data.dropFirst(3)
        } else if data.starts(with: [0xFF, 0xFE]) || data.starts(with: [0xFE, 0xFF]) {
            codec.bom = true
            codec.encoding = .utf16
        }

        var text: String?
        if codec.encoding == .utf16 {
            text = String(data: data, encoding: .utf16)
        } else if let s = String(data: body, encoding: .utf8) {
            text = s
        } else if let s = String(data: body, encoding: cp949) {
            // 한글 Windows 파일. 자동 판정보다 먼저 본다 — 판정기는 CP949 를
            // 자주 Latin-1 로 잘못 짚어 한글이 깨져 보인다.
            codec.encoding = cp949
            text = s
        } else {
            let detected = NSString.stringEncoding(for: body, encodingOptions: nil,
                                                   convertedString: nil, usedLossyConversion: nil)
            if detected != 0, let s = String(data: body, encoding: .init(rawValue: detected)) {
                codec.encoding = .init(rawValue: detected)
                text = s
            }
        }
        guard var out = text else { return nil }
        if out.contains("\r\n") {
            codec.crlf = true
            out = out.replacingOccurrences(of: "\r\n", with: "\n")
        }
        return (out, codec)
    }

    /// 저장할 데이터. 원래 인코딩으로 못 담는 글자가 있으면 nil.
    func encode(_ text: String) -> Data? {
        let body = crlf ? text.replacingOccurrences(of: "\n", with: "\r\n") : text
        guard let data = body.data(using: encoding, allowLossyConversion: false) else { return nil }
        if bom && encoding == .utf8 {
            return Data([0xEF, 0xBB, 0xBF]) + data
        }
        return data
    }
}

// MARK: - 색

enum ArgoCodeColors {
    static let text      = UIColor(white: 0.90, alpha: 1)
    static let keyword   = UIColor(red: 0.99, green: 0.47, blue: 0.66, alpha: 1)   // 분홍
    static let type      = UIColor(red: 0.40, green: 0.85, blue: 0.94, alpha: 1)   // 하늘
    static let function  = UIColor(red: 0.55, green: 0.80, blue: 1.00, alpha: 1)   // 파랑
    static let string    = UIColor(red: 0.99, green: 0.72, blue: 0.45, alpha: 1)   // 주황
    static let number    = UIColor(red: 0.82, green: 0.73, blue: 1.00, alpha: 1)   // 보라
    static let comment   = UIColor(red: 0.50, green: 0.58, blue: 0.52, alpha: 1)   // 회녹
    static let attribute = UIColor(red: 0.98, green: 0.84, blue: 0.40, alpha: 1)   // 노랑
    static let tag       = UIColor(red: 0.99, green: 0.47, blue: 0.66, alpha: 1)
    static let key       = UIColor(red: 0.55, green: 0.80, blue: 1.00, alpha: 1)
    static let error     = UIColor(red: 1.00, green: 0.42, blue: 0.40, alpha: 1)
    static let warning   = UIColor(red: 1.00, green: 0.80, blue: 0.35, alpha: 1)
    static let info      = UIColor(red: 0.45, green: 0.80, blue: 1.00, alpha: 1)
    static let dim       = UIColor(white: 0.55, alpha: 1)
    static let codeBack  = UIColor(white: 1.0, alpha: 0.07)
    static let selection = UIColor(red: 0.29, green: 0.69, blue: 0.78, alpha: 0.45)
    static let findHit   = UIColor(red: 1.00, green: 0.80, blue: 0.35, alpha: 0.40)
}

// MARK: - 코드 색칠

enum ArgoSyntax {

    private struct Rule {
        let regex: NSRegularExpression
        let color: UIColor
        /// 0 이면 전체, 아니면 그 묶음만 칠한다.
        let group: Int
    }

    private struct Grammar {
        var rules: [Rule] = []
        /// 주석·문자열. 왼쪽부터 먼저 시작한 쪽이 이긴다 —
        /// "http://" 안의 // 가 주석으로 칠해지면 안 된다.
        var protected: NSRegularExpression?
    }

    private static var cache: [ArgoLanguage: Grammar] = [:]
    private static let lock = NSLock()

    private static func re(_ p: String, _ opts: NSRegularExpression.Options = []) -> NSRegularExpression? {
        try? NSRegularExpression(pattern: p, options: opts)
    }

    private static func words(_ list: String) -> String {
        "\\b(?:" + list.split(separator: " ").joined(separator: "|") + ")\\b"
    }

    private static func grammar(_ lang: ArgoLanguage) -> Grammar {
        lock.lock(); defer { lock.unlock() }
        if let g = cache[lang] { return g }
        let g = build(lang)
        cache[lang] = g
        return g
    }

    private static func build(_ lang: ArgoLanguage) -> Grammar {
        var g = Grammar()
        func add(_ p: String, _ c: UIColor, _ group: Int = 0, _ o: NSRegularExpression.Options = []) {
            if let r = re(p, o) { g.rules.append(Rule(regex: r, color: c, group: group)) }
        }
        let number = "\\b(?:0x[0-9A-Fa-f_]+|\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)\\b"
        let dq = "\"(?:\\\\.|[^\"\\\\\\n])*\""
        let sq = "'(?:\\\\.|[^'\\\\\\n])*'"
        let bt = "`(?:\\\\.|[^`\\\\])*`"
        let slash = "//[^\\n]*"
        let block = "/\\*[\\s\\S]*?\\*/"
        let hash = "#[^\\n]*"

        // 주석이 묶음 1, 문자열이 묶음 2 다.
        func protect(comment: [String], string: [String]) {
            let c = comment.isEmpty ? "(?!)" : comment.joined(separator: "|")
            let s = string.isEmpty ? "(?!)" : string.joined(separator: "|")
            g.protected = re("(" + c + ")|(" + s + ")", [.anchorsMatchLines])
        }

        switch lang {
        case .swift:
            add(words("associatedtype class deinit enum extension fileprivate func import init inout internal let open operator private protocol public rethrows static struct subscript typealias var break case continue default defer do else fallthrough for guard if in repeat return switch where while as catch false is nil super self Self throw throws true try async await actor some any weak unowned lazy final override mutating nonmutating convenience required get set willSet didSet"), ArgoCodeColors.keyword)
            add("\\b[A-Z][A-Za-z0-9_]*\\b", ArgoCodeColors.type)
            add("\\b([a-z_][A-Za-z0-9_]*)\\s*(?=\\()", ArgoCodeColors.function, 1)
            add("@[A-Za-z_]+|#(?:if|else|elseif|endif|available|selector|keyPath|warning|error)\\b", ArgoCodeColors.attribute)
            add(number, ArgoCodeColors.number)
            protect(comment: [slash, block], string: ["\"\"\"[\\s\\S]*?\"\"\"", dq])
        case .cFamily, .java, .go, .rust:
            add(words("auto break case char const continue default do double else enum extern float for goto if inline int long register restrict return short signed sizeof static struct switch typedef union unsigned void volatile while bool true false NULL nullptr class public private protected virtual override template typename namespace using new delete this throw try catch final abstract extends implements import package interface instanceof synchronized super func var let mut fn impl pub use mod match loop where trait self Self go chan select defer map range type struct val fun object when is in as nil null id YES NO @interface @implementation @end @property @synthesize @protocol @import"), ArgoCodeColors.keyword)
            add("^\\s*#\\s*\\w+", ArgoCodeColors.attribute, 0, [.anchorsMatchLines])
            add("@[A-Za-z_]+", ArgoCodeColors.attribute)
            add("\\b[A-Z][A-Za-z0-9_]*\\b", ArgoCodeColors.type)
            add("\\b([a-z_][A-Za-z0-9_]*)\\s*(?=\\()", ArgoCodeColors.function, 1)
            add(number, ArgoCodeColors.number)
            protect(comment: [slash, block], string: [dq, sq, bt])
        case .javascript:
            add(words("break case catch class const continue debugger default delete do else export extends finally for from function if import in instanceof let new of return super switch this throw try typeof var void while with yield async await static get set true false null undefined interface type enum implements private public protected readonly as"), ArgoCodeColors.keyword)
            add("\\b[A-Z][A-Za-z0-9_]*\\b", ArgoCodeColors.type)
            add("\\b([a-zA-Z_$][\\w$]*)\\s*(?=\\()", ArgoCodeColors.function, 1)
            add(number, ArgoCodeColors.number)
            protect(comment: [slash, block], string: [dq, sq, bt])
        case .python:
            add(words("and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield self match case"), ArgoCodeColors.keyword)
            add("@[A-Za-z_][\\w.]*", ArgoCodeColors.attribute)
            add("\\b(?:def|class)\\s+([A-Za-z_]\\w*)", ArgoCodeColors.function, 1)
            add("\\b([a-zA-Z_]\\w*)\\s*(?=\\()", ArgoCodeColors.function, 1)
            add(number, ArgoCodeColors.number)
            protect(comment: [hash], string: ["\"\"\"[\\s\\S]*?\"\"\"", "'''[\\s\\S]*?'''", dq, sq])
        case .shell, .makefile:
            add(words("if then else elif fi case esac for while until do done in function return exit export local readonly declare set unset shift source alias echo printf cd test"), ArgoCodeColors.keyword)
            add("\\$\\{[^}]*\\}|\\$[A-Za-z_][A-Za-z0-9_]*|\\$[0-9@#?*!$-]", ArgoCodeColors.attribute)
            add("^[A-Za-z0-9_.-]+(?=\\s*:(?!=))", ArgoCodeColors.function, 0, [.anchorsMatchLines])
            add(number, ArgoCodeColors.number)
            protect(comment: ["(?:^|(?<=\\s))#[^\\n]*"], string: [dq, sq, bt])
        case .ruby:
            add(words("alias and begin break case class def defined do else elsif end ensure false for if in module next nil not or redo rescue retry return self super then true undef unless until when while yield require attr_accessor attr_reader"), ArgoCodeColors.keyword)
            add(":[A-Za-z_]\\w*|@{1,2}[A-Za-z_]\\w*", ArgoCodeColors.attribute)
            add("\\b[A-Z][A-Za-z0-9_]*\\b", ArgoCodeColors.type)
            add(number, ArgoCodeColors.number)
            protect(comment: [hash], string: [dq, sq])
        case .lua, .sql:
            add(words("and break do else elseif end false for function goto if in local nil not or repeat return then true until while select from where insert into values update delete create table drop alter join left right inner outer on group by order having limit as and or not null primary key index view distinct union all case when"), ArgoCodeColors.keyword, 0, [.caseInsensitive])
            add(number, ArgoCodeColors.number)
            protect(comment: ["--[^\\n]*", block], string: [dq, sq])
        case .html, .xml:
            add("</?([A-Za-z][\\w:.-]*)", ArgoCodeColors.tag, 1)
            add("\\b([A-Za-z_:][\\w:.-]*)(?==)", ArgoCodeColors.attribute, 1)
            add("&[A-Za-z#0-9]+;", ArgoCodeColors.number)
            protect(comment: ["<!--[\\s\\S]*?-->", "<!\\[CDATA\\[[\\s\\S]*?\\]\\]>"], string: [dq, sq])
        case .css:
            add("^[^{}\\n]+(?=\\{)", ArgoCodeColors.tag, 0, [.anchorsMatchLines])
            add("([A-Za-z-]+)\\s*:", ArgoCodeColors.key, 1)
            add("#[0-9A-Fa-f]{3,8}\\b|" + number + "(?:px|em|rem|%|vh|vw|s|ms|pt)?", ArgoCodeColors.number)
            add("@[A-Za-z-]+", ArgoCodeColors.keyword)
            protect(comment: [block], string: [dq, sq])
        case .json:
            add(words("true false null"), ArgoCodeColors.keyword)
            add("-?" + number, ArgoCodeColors.number)
            // 키는 문자열과 같은 모양이라, 보호 단계에서 뒤에 ':' 가 오는지로 가린다.
            protect(comment: [slash, block], string: [dq])
        case .yaml:
            add("^\\s*-?\\s*([A-Za-z0-9_.\\-\"' ]+?)\\s*:(?=\\s|$)", ArgoCodeColors.key, 1, [.anchorsMatchLines])
            add(words("true false yes no null on off"), ArgoCodeColors.keyword, 0, [.caseInsensitive])
            add("^---$|^\\.\\.\\.$", ArgoCodeColors.dim, 0, [.anchorsMatchLines])
            add("&\\w+|\\*\\w+|!\\w+", ArgoCodeColors.attribute)
            add(number, ArgoCodeColors.number)
            protect(comment: ["(?:^|(?<=\\s))#[^\\n]*"], string: [dq, sq])
        case .ini:
            add("^\\s*\\[[^\\]\\n]*\\]", ArgoCodeColors.tag, 0, [.anchorsMatchLines])
            add("^\\s*([A-Za-z0-9_.\\-/ ]+?)\\s*(?==|:)", ArgoCodeColors.key, 1, [.anchorsMatchLines])
            add(words("true false yes no on off"), ArgoCodeColors.keyword, 0, [.caseInsensitive])
            add(number, ArgoCodeColors.number)
            protect(comment: ["^\\s*[#;][^\\n]*", slash], string: [dq])
        case .markdown:
            add("^#{1,6} [^\\n]*", ArgoCodeColors.keyword, 0, [.anchorsMatchLines])
            add("^\\s*(?:[-*+]|\\d+\\.)\\s", ArgoCodeColors.attribute, 0, [.anchorsMatchLines])
            add("^>[^\\n]*", ArgoCodeColors.comment, 0, [.anchorsMatchLines])
            add("\\*\\*[^*\\n]+\\*\\*|__[^_\\n]+__", ArgoCodeColors.type)
            add("\\[[^\\]\\n]*\\]\\([^)\\n]*\\)", ArgoCodeColors.function)
            protect(comment: ["<!--[\\s\\S]*?-->"], string: ["```[\\s\\S]*?```", "`[^`\\n]+`"])
        case .log:
            add("^.*\\b(?:ERROR|ERR|FATAL|FAIL(?:ED|URE)?|CRASH|PANIC|EXCEPTION|Error|Fatal|Failed|실패|오류)\\b.*$", ArgoCodeColors.error, 0, [.anchorsMatchLines])
            add("\\b(?:WARN(?:ING)?|Warning|경고)\\b", ArgoCodeColors.warning)
            add("\\b(?:INFO|NOTICE|DEBUG|TRACE|VERBOSE)\\b", ArgoCodeColors.info)
            add("\\b\\d{4}-\\d{2}-\\d{2}[ T]\\d{2}:\\d{2}:\\d{2}(?:[.,]\\d+)?(?:Z|[+-]\\d{2}:?\\d{2})?|\\b\\d{2}:\\d{2}:\\d{2}(?:\\.\\d+)?\\b", ArgoCodeColors.dim)
            add("\\[[^\\]\\n]{1,40}\\]", ArgoCodeColors.attribute)
            protect(comment: [], string: [dq])
        case .csv:
            add(",|\\t", ArgoCodeColors.dim)
            add(number, ArgoCodeColors.number)
            protect(comment: [], string: [dq])
        case .plain:
            break
        }
        return g
    }

    /// range 안을 칠한다. range 는 attributed 의 글자 단위(UTF-16) 범위다.
    static func highlight(_ text: NSMutableAttributedString, range: NSRange, language: ArgoLanguage) {
        guard language != .plain, range.length > 0 else { return }
        let g = grammar(language)
        let s = text.string
        text.beginEditing()
        text.addAttribute(.foregroundColor, value: ArgoCodeColors.text, range: range)

        for rule in g.rules {
            rule.regex.enumerateMatches(in: s, options: [], range: range) { m, _, _ in
                guard let m else { return }
                let r = rule.group == 0 ? m.range : m.range(at: rule.group)
                if r.location != NSNotFound, r.length > 0 {
                    text.addAttribute(.foregroundColor, value: rule.color, range: r)
                }
            }
        }
        g.protected?.enumerateMatches(in: s, options: [], range: range) { m, _, _ in
            guard let m else { return }
            if m.range(at: 1).location != NSNotFound {
                text.addAttribute(.foregroundColor, value: ArgoCodeColors.comment, range: m.range(at: 1))
            } else if m.range(at: 2).location != NSNotFound {
                var color = ArgoCodeColors.string
                // JSON 키: 문자열 바로 뒤에 ':' 가 오면 키로 칠한다.
                if language == .json {
                    let end = m.range(at: 2).upperBound
                    let ns = s as NSString
                    var i = end
                    while i < ns.length, let c = UnicodeScalar(ns.character(at: i)),
                          c == " " || c == "\t" { i += 1 }
                    if i < ns.length && ns.character(at: i) == 58 /* : */ { color = ArgoCodeColors.key }
                }
                if language == .markdown {
                    color = ArgoCodeColors.string
                }
                text.addAttribute(.foregroundColor, value: color, range: m.range(at: 2))
            }
        }
        text.endEditing()
    }
}

// MARK: - 마크다운

/// .md 를 읽기 좋게 그린다. 블록(제목·목록·인용·코드·표·가로줄)을 줄 단위로
/// 가르고, 문단 안의 꾸밈(굵게·기울임·코드·링크·취소선)을 입힌다.
///
/// 웹 뷰를 쓰지 않는다. 마크다운 하나를 보려고 WebKit 을 띄우면 무겁고,
/// 리모컨 스크롤이 다른 읽기 화면과 달라진다.
enum ArgoMarkdown {

    static func render(_ source: String, baseSize: CGFloat) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let lines = source.components(separatedBy: "\n")
        var i = 0

        func para(indent: CGFloat = 0, first: CGFloat? = nil, spacingBefore: CGFloat = 0,
                  spacingAfter: CGFloat? = nil) -> NSMutableParagraphStyle {
            let p = NSMutableParagraphStyle()
            p.lineSpacing = baseSize * 0.3
            p.paragraphSpacing = spacingAfter ?? baseSize * 0.55
            p.paragraphSpacingBefore = spacingBefore
            p.headIndent = indent
            p.firstLineHeadIndent = first ?? indent
            p.lineBreakMode = .byWordWrapping
            return p
        }
        let body = UIFont.systemFont(ofSize: baseSize)
        let mono = UIFont.monospacedSystemFont(ofSize: baseSize * 0.9, weight: .regular)
        let baseAttrs: [NSAttributedString.Key: Any] = [.font: body, .foregroundColor: ArgoCodeColors.text]

        func appendLine(_ a: NSAttributedString) {
            out.append(a)
            out.append(NSAttributedString(string: "\n", attributes: [.font: body]))
        }

        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            // 코드 블록
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                let fence = String(trimmed.prefix(3))
                let info = trimmed.dropFirst(3).trimmingCharacters(in: .whitespaces).lowercased()
                var code: [String] = []
                i += 1
                while i < lines.count && !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    code.append(lines[i]); i += 1
                }
                i += 1
                let block = NSMutableAttributedString(string: code.joined(separator: "\n"), attributes: [
                    .font: mono, .foregroundColor: ArgoCodeColors.text,
                    .backgroundColor: ArgoCodeColors.codeBack,
                    .paragraphStyle: para(indent: baseSize, spacingAfter: baseSize * 0.2)
                ])
                let lang = ArgoTextKind.language(for: "x." + (info.isEmpty ? "txt" : alias(info)))
                ArgoSyntax.highlight(block, range: NSRange(location: 0, length: block.length), language: lang)
                appendLine(block)
                out.append(NSAttributedString(string: "\n", attributes: [.font: UIFont.systemFont(ofSize: baseSize * 0.4)]))
                continue
            }

            // 빈 줄
            if trimmed.isEmpty { i += 1; continue }

            // HTML 주석은 숨긴다.
            if trimmed.hasPrefix("<!--") {
                while i < lines.count && !lines[i].contains("-->") { i += 1 }
                i += 1
                continue
            }

            // 가로줄
            if trimmed.count >= 3, Set(trimmed.replacingOccurrences(of: " ", with: "")).count == 1,
               let c = trimmed.first, "-*_".contains(c) {
                appendLine(NSAttributedString(string: String(repeating: "─", count: 40), attributes: [
                    .font: body, .foregroundColor: ArgoCodeColors.dim, .paragraphStyle: para()
                ]))
                i += 1
                continue
            }

            // 제목
            if let level = headingLevel(trimmed) {
                let text = String(trimmed.dropFirst(level)).trimmingCharacters(in: .whitespaces)
                    .trimmingCharacters(in: CharacterSet(charactersIn: "#")).trimmingCharacters(in: .whitespaces)
                let scale: [CGFloat] = [1.9, 1.55, 1.3, 1.15, 1.05, 1.0]
                let size = baseSize * scale[level - 1]
                let a = inline(text, font: UIFont.systemFont(ofSize: size, weight: level <= 2 ? .bold : .semibold),
                               mono: mono, color: level <= 2 ? UIColor.white : ArgoCodeColors.text)
                a.addAttribute(.paragraphStyle, value: para(spacingBefore: baseSize * (level <= 2 ? 0.9 : 0.5),
                                                            spacingAfter: baseSize * 0.4),
                               range: NSRange(location: 0, length: a.length))
                appendLine(a)
                if level <= 2 {
                    appendLine(NSAttributedString(string: String(repeating: "─", count: 40), attributes: [
                        .font: UIFont.systemFont(ofSize: baseSize * 0.5), .foregroundColor: ArgoCodeColors.dim
                    ]))
                }
                i += 1
                continue
            }

            // 표: 다음 줄이 |---| 모양이면 표로 본다.
            if line.contains("|"), i + 1 < lines.count, isTableRule(lines[i + 1]) {
                var rows: [[String]] = [cells(line)]
                i += 2
                while i < lines.count, lines[i].contains("|"),
                      !lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
                    rows.append(cells(lines[i])); i += 1
                }
                appendLine(table(rows, mono: mono, para: para(indent: baseSize * 0.5)))
                continue
            }

            // 인용
            if trimmed.hasPrefix(">") {
                var quote: [String] = []
                while i < lines.count {
                    let t = lines[i].trimmingCharacters(in: .whitespaces)
                    guard t.hasPrefix(">") else { break }
                    quote.append(String(t.dropFirst()).trimmingCharacters(in: .whitespaces))
                    i += 1
                }
                let a = NSMutableAttributedString(string: "▍ ", attributes: [.font: body, .foregroundColor: ArgoCodeColors.function])
                a.append(inline(quote.joined(separator: " "), font: UIFont.italicSystemFont(ofSize: baseSize),
                                mono: mono, color: ArgoCodeColors.dim))
                a.addAttribute(.paragraphStyle, value: para(indent: baseSize * 1.6, first: baseSize * 0.6),
                               range: NSRange(location: 0, length: a.length))
                appendLine(a)
                continue
            }

            // 목록
            if let item = listItem(line) {
                let depth = CGFloat(item.depth)
                let a = NSMutableAttributedString(string: item.marker + " ", attributes: [
                    .font: item.ordered ? body : UIFont.systemFont(ofSize: baseSize, weight: .bold),
                    .foregroundColor: ArgoCodeColors.function
                ])
                a.append(inline(item.text, font: body, mono: mono, color: ArgoCodeColors.text))
                let lead = baseSize * (1.2 + depth * 1.4)
                a.addAttribute(.paragraphStyle,
                               value: para(indent: lead + baseSize * 1.1, first: lead, spacingAfter: baseSize * 0.25),
                               range: NSRange(location: 0, length: a.length))
                appendLine(a)
                i += 1
                continue
            }

            // 문단: 빈 줄이나 다른 블록이 나올 때까지 이어 붙인다.
            var parts: [String] = []
            while i < lines.count {
                let l = lines[i]
                let t = l.trimmingCharacters(in: .whitespaces)
                if t.isEmpty || t.hasPrefix("#") || t.hasPrefix(">") || t.hasPrefix("```")
                    || t.hasPrefix("~~~") || listItem(l) != nil
                    || (l.contains("|") && i + 1 < lines.count && isTableRule(lines[i + 1])) { break }
                // 줄 끝 공백 둘은 줄바꿈이다.
                parts.append(l.hasSuffix("  ") ? t + "\n" : t)
                i += 1
            }
            if parts.isEmpty { parts.append(trimmed); i += 1 }
            let joined = parts.joined(separator: " ").replacingOccurrences(of: "\n ", with: "\n")
            let a = inline(joined, font: body, mono: mono, color: ArgoCodeColors.text)
            a.addAttribute(.paragraphStyle, value: para(), range: NSRange(location: 0, length: a.length))
            appendLine(a)
        }
        if out.length == 0 { out.append(NSAttributedString(string: "(빈 문서)", attributes: baseAttrs)) }
        return out
    }

    private static func alias(_ info: String) -> String {
        switch info {
        case "javascript", "typescript", "ts", "node": return "js"
        case "python", "python3": return "py"
        case "bash", "shell", "console", "zsh", "sh": return "sh"
        case "objc", "objective-c", "c++", "cpp": return "m"
        case "yml": return "yaml"
        case "html", "xml", "json", "swift", "css", "go", "rust", "java", "sql", "lua", "ruby": return info == "rust" ? "rs" : (info == "ruby" ? "rb" : info)
        default: return info
        }
    }

    private static func headingLevel(_ t: String) -> Int? {
        let n = t.prefix(while: { $0 == "#" }).count
        guard (1...6).contains(n) else { return nil }
        let rest = t.dropFirst(n)
        return rest.isEmpty || rest.first == " " ? n : nil
    }

    private static func isTableRule(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.contains("-"), t.contains("|") || t.hasPrefix("-") else { return false }
        return t.allSatisfy { "|-: ".contains($0) }
    }

    private static func cells(_ line: String) -> [String] {
        var t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("|") { t.removeFirst() }
        if t.hasSuffix("|") { t.removeLast() }
        return t.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
    }

    /// 한글·한자는 고정폭 글꼴에서도 두 칸을 차지한다. 칸을 맞출 때 그만큼 센다.
    private static func width(_ s: String) -> Int {
        s.unicodeScalars.reduce(0) { n, u in
            let v = u.value
            let wide = (0x1100...0x115F).contains(v) || (0x2E80...0xA4CF).contains(v)
                || (0xAC00...0xD7A3).contains(v) || (0xF900...0xFAFF).contains(v)
                || (0xFF00...0xFF60).contains(v) || (0x1F300...0x1FAFF).contains(v)
            return n + (wide ? 2 : 1)
        }
    }

    private static func table(_ rows: [[String]], mono: UIFont, para: NSParagraphStyle) -> NSAttributedString {
        let cols = rows.map { $0.count }.max() ?? 0
        var widths = [Int](repeating: 0, count: cols)
        let plain = rows.map { $0.map { stripInline($0) } }
        for r in plain { for (j, c) in r.enumerated() { widths[j] = max(widths[j], width(c)) } }

        let out = NSMutableAttributedString()
        for (ri, r) in plain.enumerated() {
            var line = ""
            for j in 0..<cols {
                let c = j < r.count ? r[j] : ""
                line += c + String(repeating: " ", count: max(0, widths[j] - width(c)))
                if j < cols - 1 { line += "  │  " }
            }
            out.append(NSAttributedString(string: line, attributes: [
                .font: ri == 0 ? UIFont.monospacedSystemFont(ofSize: mono.pointSize, weight: .bold) : mono,
                .foregroundColor: ri == 0 ? UIColor.white : ArgoCodeColors.text,
                .paragraphStyle: para
            ]))
            out.append(NSAttributedString(string: "\n"))
            if ri == 0 {
                let total = widths.reduce(0, +) + max(0, cols - 1) * 5
                out.append(NSAttributedString(string: String(repeating: "─", count: total) + "\n", attributes: [
                    .font: mono, .foregroundColor: ArgoCodeColors.dim, .paragraphStyle: para
                ]))
            }
        }
        if out.length > 0 { out.deleteCharacters(in: NSRange(location: out.length - 1, length: 1)) }
        return out
    }

    private static func listItem(_ line: String) -> (depth: Int, marker: String, text: String, ordered: Bool)? {
        let spaces = line.prefix(while: { $0 == " " || $0 == "\t" })
        let depth = spaces.reduce(0) { $0 + ($1 == "\t" ? 2 : 1) } / 2
        let t = line.dropFirst(spaces.count)
        if let first = t.first, "-*+".contains(first), t.dropFirst().first == " " {
            var text = String(t.dropFirst(2))
            var marker = depth == 0 ? "•" : (depth == 1 ? "◦" : "▪")
            if text.hasPrefix("[ ] ") { marker = "☐"; text = String(text.dropFirst(4)) }
            else if text.lowercased().hasPrefix("[x] ") { marker = "☑"; text = String(text.dropFirst(4)) }
            return (depth, marker, text, false)
        }
        let digits = t.prefix(while: { $0.isNumber })
        if !digits.isEmpty, digits.count < 4 {
            let after = t.dropFirst(digits.count)
            if let p = after.first, p == "." || p == ")", after.dropFirst().first == " " {
                return (depth, String(digits) + ".", String(after.dropFirst(2)), true)
            }
        }
        return nil
    }

    private static func stripInline(_ s: String) -> String {
        var t = s
        for (p, r) in [("\\*\\*([^*]+)\\*\\*", "$1"), ("__([^_]+)__", "$1"), ("`([^`]+)`", "$1"),
                       ("\\[([^\\]]*)\\]\\([^)]*\\)", "$1"), ("\\*([^*]+)\\*", "$1")] {
            t = t.replacingOccurrences(of: p, with: r, options: .regularExpression)
        }
        return t
    }

    /// 문단 안 꾸밈. 코드 조각을 먼저 떼어 두어, 그 안의 * 나 _ 가 꾸밈으로
    /// 먹히지 않게 한다.
    static func inline(_ text: String, font: UIFont, mono: UIFont, color: UIColor) -> NSMutableAttributedString {
        let a = NSMutableAttributedString(string: text, attributes: [.font: font, .foregroundColor: color])

        func apply(_ pattern: String, _ transform: (NSMutableAttributedString, NSRange, String) -> Void) {
            guard let re = try? NSRegularExpression(pattern: pattern) else { return }
            let matches = re.matches(in: a.string, range: NSRange(location: 0, length: a.length))
            for m in matches.reversed() {
                // 이미 코드로 칠한 곳은 건드리지 않는다.
                if a.attribute(.backgroundColor, at: m.range.location, effectiveRange: nil) != nil { continue }
                let inner = m.numberOfRanges > 1 ? (a.string as NSString).substring(with: m.range(at: 1)) : ""
                transform(a, m.range, inner)
            }
        }
        func trait(_ f: UIFont, _ t: UIFontDescriptor.SymbolicTraits) -> UIFont {
            guard let d = f.fontDescriptor.withSymbolicTraits(f.fontDescriptor.symbolicTraits.union(t)) else { return f }
            return UIFont(descriptor: d, size: f.pointSize)
        }

        // 코드
        apply("`([^`]+)`") { a, r, inner in
            a.replaceCharacters(in: r, with: NSAttributedString(string: " " + inner + " ", attributes: [
                .font: mono, .foregroundColor: ArgoCodeColors.string, .backgroundColor: ArgoCodeColors.codeBack
            ]))
        }
        // 그림: 글로만 알린다.
        apply("!\\[([^\\]]*)\\]\\([^)]*\\)") { a, r, inner in
            a.replaceCharacters(in: r, with: NSAttributedString(string: "[그림: \(inner)]", attributes: [
                .font: font, .foregroundColor: ArgoCodeColors.dim
            ]))
        }
        // 링크: 글만 남기고 색을 입힌다. 주소는 TV 에서 열 길이 없다.
        apply("\\[([^\\]]+)\\]\\(([^)]*)\\)") { a, r, inner in
            a.replaceCharacters(in: r, with: NSAttributedString(string: inner, attributes: [
                .font: font, .foregroundColor: ArgoCodeColors.function,
                .underlineStyle: NSUnderlineStyle.single.rawValue
            ]))
        }
        // 굵게
        for p in ["\\*\\*([^*\\n]+)\\*\\*", "__([^_\\n]+)__"] {
            apply(p) { a, r, inner in
                let f = (a.attribute(.font, at: r.location, effectiveRange: nil) as? UIFont) ?? font
                a.replaceCharacters(in: r, with: NSAttributedString(string: inner, attributes: [
                    .font: trait(f, .traitBold), .foregroundColor: UIColor.white
                ]))
            }
        }
        // 취소선
        apply("~~([^~\\n]+)~~") { a, r, inner in
            a.replaceCharacters(in: r, with: NSAttributedString(string: inner, attributes: [
                .font: font, .foregroundColor: ArgoCodeColors.dim,
                .strikethroughStyle: NSUnderlineStyle.single.rawValue
            ]))
        }
        // 기울임 — 낱말 가운데의 _ (snake_case) 는 건드리지 않는다.
        for p in ["(?<![*\\w])\\*([^*\\n]+)\\*(?![*\\w])", "(?<![_\\w])_([^_\\n]+)_(?![_\\w])"] {
            apply(p) { a, r, inner in
                let f = (a.attribute(.font, at: r.location, effectiveRange: nil) as? UIFont) ?? font
                let c = a.attribute(.foregroundColor, at: r.location, effectiveRange: nil) ?? color
                a.replaceCharacters(in: r, with: NSAttributedString(string: inner, attributes: [
                    .font: trait(f, .traitItalic), .foregroundColor: c
                ]))
            }
        }
        // <br>
        apply("<br\\s*/?>") { a, r, _ in
            a.replaceCharacters(in: r, with: NSAttributedString(string: "\n", attributes: [.font: font]))
        }
        return a
    }
}
