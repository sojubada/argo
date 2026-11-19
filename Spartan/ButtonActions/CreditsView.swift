//
//  CreditsView.swift
//  Argo
//
//  만든 사람들 · 출처 · 라이선스. 내용은 README.md 의 'Upstream sources' 표와 맞춘다.
//  리모컨으로 아래까지 내려갈 수 있게 문단마다 포커스를 받는다.
//

import SwiftUI

struct CreditsView: View {
	private var version: String {
		let v = (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? ""
		let b = (Bundle.main.infoDictionary?["CFBundleVersion"] as? String) ?? ""
		return "v\(v) (build \(b))"
	}

	private struct Source: Identifiable {
		let id = UUID()
		let name: String
		let use: String
		let license: String
	}

	// 가져다 쓴 코드·글꼴·서비스. 라이선스 원문은 각 폴더의 LICENSE·Cargo.toml 에 그대로 있다.
	private let sources: [Source] = [
		Source(name: "Spartan — WhitetailAni", use: L("앱 바탕(파일 관리자)"), license: "MIT"),
		Source(name: "PrivateKits-tvOS — WhitetailAni", use: L("설치된 앱 목록"), license: "Repository license"),
		Source(name: "TrollStore — opa334", use: L("루트 도우미(TSUtil)"), license: "MIT"),
		Source(name: "Pogo — Amy While", use: L("명령 실행"), license: "Original author"),
		Source(name: "Zip — marmelroy", use: L("압축 풀기"), license: "MIT"),
		Source(name: "SwiftTryCatch — William Falcon", use: L("Objective-C 예외 처리"), license: "MIT"),
		Source(name: "rhwp — edwardkim", use: L("한글 hwp/hwpx 그리기 · EMF/WMF 변환"), license: "MIT"),
		Source(name: "hwpers — Indosaram", use: L("예비 한글 엔진"), license: "MIT / Apache-2.0"),
		Source(name: "MiniPdf — mini-software", use: "Word/Excel → PDF", license: "Apache-2.0"),
		Source(name: "resvg family — linebender", use: "usvg · svg2pdf · pdf-writer · ttf-parser · rustybuzz", license: "MIT / Apache-2.0"),
		Source(name: "encoding_rs — Mozilla", use: L("옛 한글(EUC-KR) 글꼴 이름 읽기"), license: "MIT / Apache-2.0"),
		Source(name: "ECMA-376 presetShapeDefinitions", use: L("PowerPoint 도형 정의"), license: "ECMA standard"),
		Source(name: "Nanum fonts — NAVER", use: L("나눔스퀘어 · 나눔고딕 (기본 제공 글꼴)"), license: "SIL OFL 1.1"),
		Source(name: "Pretendard — Kil Hyung-jin", use: L("기본 제공 글꼴"), license: "SIL OFL 1.1"),
		Source(name: "Liberation Sans — Red Hat", use: L("Arial 대체 글꼴"), license: "SIL OFL 1.1"),
		Source(name: "SF Mono — Apple", use: L("고정폭 글꼴(16진수·글 보기)"), license: "Apple"),
		Source(name: "iTunes Search API · LRCLIB", use: L("앨범 그림·가사 찾기 (끌 수 있음)"), license: "Each service's terms"),
	]

	var body: some View {
		ScrollView {
			VStack(spacing: 18) {
				Text("Argo")
					.font(.system(size: 64, weight: .bold))
				Text(version)
					.foregroundColor(.secondary)
				Text(L("애플TV용 파일 탐색기 · 문서/미디어 뷰어"))
					.font(.system(size: 28))
					.focusable()

				section(L("만든 사람"), """
				Rob (sojubada) — \(L("기획 · 개발 · 검증"))
				Claude (Anthropic) — \(L("AI 공동 개발(코드 작성·문서)"))
				""")

				section(L("바탕이 된 프로젝트"), """
				Spartan © 2023 WhitetailAni (MIT)
				github.com/WhitetailAni/Spartan
				\(L("Spartan 기여자")): SerenaKit, staturnz, ethanrdoesmc, llsc12, flower, trebrick
				""")

				Text(L("가져다 쓴 코드 · 글꼴 · 서비스"))
					.font(.system(size: 34, weight: .semibold))
					.padding(.top, 12)
				ForEach(sources) { s in
					VStack(spacing: 2) {
						Text(s.name).font(.system(size: 26, weight: .medium))
						Text("\(s.use) · \(s.license)")
							.font(.system(size: 22))
							.foregroundColor(.secondary)
					}
					.focusable()
				}

				section(L("직접 만든 부분"), L("PowerPoint 그리기(rust/argoconv/slides)와 tvOS 화면은 이 저장소에서 새로 썼습니다."))
				section(L("라이선스"), L("Argo는 MIT 라이선스입니다. 가져온 구성 요소는 각자의 라이선스를 따르며, 원문은 각 폴더의 LICENSE에 그대로 있습니다."))
				Text("github.com/sojubada/argo")
					.font(.system(size: 24))
					.foregroundColor(.secondary)
					.focusable()
			}
			.multilineTextAlignment(.center)
			.padding(40)
		}
	}

	private func section(_ title: String, _ body: String) -> some View {
		VStack(spacing: 6) {
			Text(title).font(.system(size: 34, weight: .semibold))
			Text(body).font(.system(size: 26))
		}
		.padding(.top, 12)
		.focusable()
	}
}
