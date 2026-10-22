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
    let directory: String
    let onConfirm: () -> Void
    let onCancel: () -> Void
    
    var body: some View {
        ZStack {
            Color.black.opacity(0.85).edgesIgnoringSafeArea(.all)
            
            VStack(spacing: 20) {
                Text("Delete \(files.count) Items?")
                    .font(.largeTitle)
                    .foregroundColor(.red)
                
                ScrollView {
                    VStack(alignment: .leading, spacing: 5) {
                        ForEach(files.prefix(10), id: \.self) { file in
                            Text("• \(file)")
                                .foregroundColor(.white)
                        }
                        if files.count > 10 {
                            Text("... and \(files.count - 10) more.")
                                .foregroundColor(.gray)
                        }
                    }
                }
                .frame(maxHeight: 300)
                
                let cautionCount = files.filter { PathSafety.risk(of: directory + $0) == .caution }.count
                if cautionCount > 0 {
                    Text("⚠️ \(cautionCount) item(s) are in a Caution zone.")
                        .foregroundColor(.orange)
                }
                
                HStack(spacing: 40) {
                    Button("Cancel", action: onCancel)
                    Button(action: onConfirm) {
                        Text("Delete / Move to Trash")
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