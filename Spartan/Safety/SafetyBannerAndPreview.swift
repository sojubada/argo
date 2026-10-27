//
//  SafetyBannerAndPreview.swift
//  Argo
//

import SwiftUI

struct PathSafetyBanner: View {
    let risk: PathRisk
    let message: String
    
    var body: some View {
        VStack {
            HStack {
                Image(systemName: risk == .forbidden ? "lock.fill" : "exclamationmark.triangle.fill")
                Text(message)
                    .font(.headline)
            }
            .padding()
            .background(risk == .forbidden ? Color.red.opacity(0.9) : Color.orange.opacity(0.9))
            .foregroundColor(.white)
            .cornerRadius(12)
            .shadow(radius: 10)
            .padding(.top, 40)
            
            Spacer()
        }
        .allowsHitTesting(false) // 포커스 뺏지 않음
    }
}

struct BatchDeletePreview: View {
    let files: [String]
    let cautionCount: Int
    let onConfirm: () -> Void
    let onCancel: () -> Void

    private enum Field { case cancel, confirm }
    /// 뜨자마자 '취소' 에 포커스를 둔다.
    ///
    /// 지우는 화면에서 포커스가 어디에도 없으면 리모컨을 아무리 눌러도
    /// 반응이 없다. 그리고 처음 놓이는 자리는 취소여야 한다 — 한 번 더
    /// 누르는 것만으로 지워지면 안 된다.
    @FocusState private var focus: Field?

    var body: some View {
        ZStack {
            Color.black.opacity(0.85).edgesIgnoringSafeArea(.all)
            
            VStack(spacing: 20) {
                Text("지울 것 \(files.count)개")
                    .font(.largeTitle)
                    .foregroundColor(.red)
                
                ScrollView {
                    VStack(alignment: .leading, spacing: 5) {
                        ForEach(files.prefix(10), id: \.self) { file in
                            Text("• \(file)")
                                .foregroundColor(.white)
                        }
                        if files.count > 10 {
                            Text("...외 \(files.count - 10)개")
                                .foregroundColor(.gray)
                        }
                    }
                }
                .frame(maxHeight: 300)
                
                if cautionCount > 0 {
                    Text("⚠️ \(cautionCount)개가 주의 구역에 있습니다")
                        .foregroundColor(.orange)
                }
                
                HStack(spacing: 40) {
                    Button("취소", action: onCancel)
                        .focused($focus, equals: .cancel)
                    Button(action: onConfirm) {
                        Text("휴지통으로 / 삭제")
                            .foregroundColor(.red)
                    }
                    .focused($focus, equals: .confirm)
                }
                // 두 단추를 한 묶음으로 둔다. 좌우로 오갈 때 목록 쪽으로
                // 새지 않는다.
                .focusSection()
            }
            .padding(40)
            .background(Color(white: 0.15))
            .cornerRadius(20)
        }
        .onAppear { focus = .cancel }
    }
}