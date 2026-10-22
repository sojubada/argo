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
                    Button(action: onConfirm) {
                        Text("휴지통으로 / 삭제")
                            .foregroundColor(.red)
                    }
                }
            }
            .padding(40)
            .background(Color(white: 0.15))
            .cornerRadius(20)
        }
    }
}