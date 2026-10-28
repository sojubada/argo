//
//  CompressionRunner.swift
//  Argo
//
//  압축과 해제를 뒷줄에서 돌린다.
//
//  전에는 버튼 액션 안에서 바로 돌았다. 10GB 파일이면 메인 스레드가 몇 분
//  막히고, 화면이 통째로 멈추며 리모컨도 안 먹는다. tvOS 가 응답 없는 앱을
//  죽이면 반쪽 아카이브가 남는다.
//
//  진행률은 없다. libarchive 와 minizip 을 거치는 호출이 중간에 아무것도
//  알려주지 않기 때문이다. 측정할 수 없는 것에 가짜 퍼센트를 붙이지 않는다 —
//  무엇을 하는 중인지만 말한다.
//
//  취소도 같은 이유로 '요청'까지만 한다. 아래로 내려간 C 호출을 중간에
//  끊을 방법이 없으므로, 취소는 여러 파일을 다룰 때 다음 파일로 넘어가지
//  않게 하는 것까지다. 할 수 있는 것과 없는 것을 그대로 말한다.
//

import Foundation
import SwiftUI

@available(tvOS 13.0, *)
final class CompressionRunner: ObservableObject {

    @Published var isRunning = false
    /// 지금 무엇을 하는 중인지. 사용자가 읽을 문장이다.
    @Published var message = ""
    /// 끝난 뒤의 오류. nil 이면 성공이다.
    @Published var failure: Error?
    /// 취소를 요청했는가. 이미 내려간 호출은 끊지 못한다.
    @Published private(set) var cancelRequested = false

    func requestCancel() {
        cancelRequested = true
        message = L("현재 파일을 마치는 중입니다…")
    }

    /// 뒷줄에서 돌리고 끝나면 메인에서 알린다.
    /// onFinish 는 성공일 때만 불린다 — 실패는 failure 로 간다.
    func run(_ what: String,
             work: @escaping () throws -> Void,
             onFinish: @escaping () -> Void) {
        guard !isRunning else { return }
        isRunning = true
        cancelRequested = false
        failure = nil
        message = what

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var thrown: Error?
            do {
                try work()
            } catch {
                thrown = error
            }
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.isRunning = false
                self.message = ""
                if let thrown = thrown {
                    self.failure = thrown
                } else {
                    onFinish()
                }
            }
        }
    }
}

/// 작업 중 화면. 진행률 막대가 없는 이유는 위에 적었다.
@available(tvOS 13.0, *)
struct CompressionProgressView: View {
    @ObservedObject var runner: CompressionRunner
    let fileName: String

    var body: some View {
        VStack(spacing: 20) {
            ProgressView()
            Text(runner.message)
                .font(.title3)
            Text(fileName)
                .font(.caption)
                .foregroundColor(Theme.textSecondary)
                .lineLimit(1)
                .truncationMode(.middle)

            // 큰 파일은 몇 분이 걸린다. 멈춘 것이 아니라고 말해 둔다.
            Text(L("큰 파일은 몇 분이 걸릴 수 있습니다. 화면을 끄지 마십시오."))
                .font(.caption)
                .foregroundColor(Theme.textDisabled)
                .multilineTextAlignment(.center)

            if !runner.cancelRequested {
                Button(action: { runner.requestCancel() }) {
                    Text(L("취소"))
                }
            }
        }
        .padding(40)
        .frame(maxWidth: 800)
    }
}
