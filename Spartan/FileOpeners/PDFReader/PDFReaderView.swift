//
//  PDFReaderView.swift
//  Argo
//
//  Simple PDF Reader View:
//  - Read (1 Page / 2 Pages Book Spread)
//  - Night Mode (Inverted)
//  - 3D Book Page Flip Animation
//

import SwiftUI
import CoreGraphics
import UIKit

struct PDFReaderView: View {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    @StateObject private var model: PDFReaderModel
    @State private var showToolbar: Bool = false
    @State private var toolbarAutoHideTask: DispatchWorkItem? = nil

    @State private var passwordInput: String = ""
    @State private var passwordError: String = ""

    @FocusState private var canvasFocused: Bool

    init(filePath: String, fileName: String, isPresented: Binding<Bool>) {
        self.filePath = filePath
        self.fileName = fileName
        self._isPresented = isPresented
        self._model = StateObject(wrappedValue: PDFReaderModel(filePath: filePath, fileName: fileName))
    }

    var body: some View {
        ZStack {
            (model.isNightMode ? Color.black : Color(white: 0.14))
                .edgesIgnoringSafeArea(.all)

            if let err = model.loadError {
                errorView(message: err)
            } else if model.isLocked {
                lockedPasswordView
            } else {
                readerBody
            }
        }
        .onExitCommand {
            if showToolbar {
                toolbarAutoHideTask?.cancel()
                withAnimation {
                    showToolbar = false
                }
            } else {
                toolbarAutoHideTask?.cancel()
                isPresented = false
            }
        }
    }

    // MARK: - Error & Password Views

    private func errorView(message: String) -> some View {
        VStack(spacing: 24) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.largeTitle)
                .foregroundColor(.yellow)
            Text("Unable to Open PDF")
                .font(.title2)
            Text(message)
                .font(.headline)
                .foregroundColor(.secondary)
            Button("Close") {
                isPresented = false
            }
        }
        .padding(40)
    }

    private var lockedPasswordView: some View {
        VStack(spacing: 24) {
            Image(systemName: "lock.doc.fill")
                .font(.largeTitle)
            Text("Password Protected PDF")
                .font(.title2)
            SecureField("Password", text: $passwordInput)
                .frame(maxWidth: 600)
            if !passwordError.isEmpty {
                Text(passwordError)
                    .font(.caption)
                    .foregroundColor(.red)
            }
            HStack(spacing: 24) {
                Button("Unlock") {
                    if model.unlock(password: passwordInput) {
                        passwordError = ""
                        passwordInput = ""
                    } else {
                        passwordError = "Incorrect password."
                    }
                }
                Button("Cancel") {
                    isPresented = false
                }
            }
        }
        .padding(40)
    }

    // MARK: - Main Reader Body

    private var readerBody: some View {
        ZStack {
            // 책 페이지 영역 + 야간 모드 + 책장 넘기기 3D 효과
            bookSpreadCanvas
                .rotation3DEffect(
                    .degrees(model.pageTurnAngle),
                    axis: (x: 0.0, y: 1.0, z: 0.0),
                    anchor: model.pageTurnDirection >= 0 ? .leading : .trailing,
                    perspective: 0.45
                )
                .edgesIgnoringSafeArea(.all)

            // 툴바가 숨겨져 있을 때 리모컨 입력을 받는 투명 버튼
            if !showToolbar {
                remoteControlCatcher
            }

            // 상단 간결한 툴바 & 하단 페이지 표시
            VStack {
                if showToolbar {
                    simpleTopToolbar
                        .transition(.opacity)
                }

                Spacer()

                if let banner = model.resumeBannerText {
                    Text(banner)
                        .font(.headline)
                        .padding(.horizontal, 24)
                        .padding(.vertical, 12)
                        .background(Color.black.opacity(0.8))
                        .cornerRadius(12)
                        .padding(.bottom, 12)
                }

                bottomPageBadge
            }
            .padding(48)
        }
    }

    @ViewBuilder
    private var bookSpreadCanvas: some View {
        if model.isNightMode {
            rawBookSpread
                .colorInvert()
                .hueRotation(.degrees(180))
        } else {
            rawBookSpread
        }
    }

    private var rawBookSpread: some View {
        ZStack {
            if let leftImage = model.primaryPageImage {
                if model.layoutMode == .twoPagesBook {
                    HStack(spacing: 0) {
                        // 왼쪽 페이지
                        Image(uiImage: leftImage)
                            .resizable()
                            .scaledToFit()
                            .frame(maxWidth: 910, maxHeight: 1020)
                            .shadow(color: Color.black.opacity(0.35), radius: 8, x: -4, y: 2)

                        // 책 중앙 제본선(Spine) 음영 효과
                        LinearGradient(
                            gradient: Gradient(colors: [
                                Color.black.opacity(0.25),
                                Color.black.opacity(0.55),
                                Color.black.opacity(0.25)
                            ]),
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                        .frame(width: 14, height: 980)

                        // 오른쪽 페이지
                        if let rightImage = model.secondaryPageImage {
                            Image(uiImage: rightImage)
                                .resizable()
                                .scaledToFit()
                                .frame(maxWidth: 910, maxHeight: 1020)
                                .shadow(color: Color.black.opacity(0.35), radius: 8, x: 4, y: 2)
                        } else {
                            Rectangle()
                                .fill(Color.white.opacity(0.04))
                                .frame(width: 910, height: 980)
                        }
                    }
                } else {
                    // 1페이지 보기
                    Image(uiImage: leftImage)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: 1820, maxHeight: 1020)
                        .shadow(color: Color.black.opacity(0.4), radius: 12, x: 0, y: 4)
                }
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var remoteControlCatcher: some View {
        Button(action: {
            showToolbarWithAutoHide()
        }) {
            Color.clear
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focused($canvasFocused)
        .onMoveCommand { direction in
            switch direction {
            case .left:
                model.previousPageWithCurl()
            case .right:
                model.nextPageWithCurl()
            case .up, .down:
                showToolbarWithAutoHide()
            @unknown default:
                break
            }
        }
        .onPlayPauseCommand {
            model.toggleNightMode()
        }
        .onAppear {
            canvasFocused = true
        }
    }

    private func showToolbarWithAutoHide() {
        withAnimation {
            showToolbar = true
        }
        scheduleAutoHide()
    }

    private func scheduleAutoHide() {
        toolbarAutoHideTask?.cancel()
        let task = DispatchWorkItem {
            withAnimation {
                showToolbar = false
            }
        }
        toolbarAutoHideTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.0, execute: task)
    }

    // MARK: - Simple Top Toolbar

    private var simpleTopToolbar: some View {
        VStack(spacing: 12) {
            Text(fileName)
                .font(.headline)
                .lineLimit(1)

            HStack(spacing: 20) {
                Button(action: {
                    scheduleAutoHide()
                    model.jumpToPage(0)
                }) {
                    Text("First")
                }
                .disabled(!model.canGoPrevious)

                Button(action: {
                    scheduleAutoHide()
                    model.previousPageWithCurl()
                }) {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left")
                        Text("Prev")
                    }
                }
                .disabled(!model.canGoPrevious)

                Button(action: {
                    scheduleAutoHide()
                    model.nextPageWithCurl()
                }) {
                    HStack(spacing: 6) {
                        Text("Next")
                        Image(systemName: "chevron.right")
                    }
                }
                .disabled(!model.canGoNext)

                Button(action: {
                    scheduleAutoHide()
                    model.jumpToPage(model.totalPages - 1)
                }) {
                    Text("Last")
                }
                .disabled(!model.canGoNext)

                // 1쪽 / 2쪽(책) 전환
                Button(action: {
                    scheduleAutoHide()
                    model.toggleLayoutMode()
                }) {
                    HStack(spacing: 8) {
                        Image(systemName: model.layoutMode == .twoPagesBook ? "book.fill" : "doc.text")
                        Text(model.layoutMode.rawValue)
                    }
                }

                // 주간 / 야간 읽기 모드 전환
                Button(action: {
                    scheduleAutoHide()
                    model.toggleNightMode()
                }) {
                    HStack(spacing: 8) {
                        Image(systemName: model.isNightMode ? "moon.fill" : "sun.max.fill")
                        Text(model.isNightMode ? "Night" : "Day")
                    }
                }

                // 닫기
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    isPresented = false
                }) {
                    HStack(spacing: 6) {
                        Image(systemName: "xmark")
                        Text("Close")
                    }
                }
            }
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 16)
        .background(Color.black.opacity(0.85))
        .cornerRadius(16)
    }

    private var bottomPageBadge: some View {
        HStack(spacing: 16) {
            Image(systemName: model.layoutMode == .twoPagesBook ? "book" : "doc")
                .font(.caption)
            Text(model.pageStatusText)
                .font(.caption)
            if model.isNightMode {
                Image(systemName: "moon.fill")
                    .font(.caption)
                    .foregroundColor(.yellow)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 8)
        .background(Color.black.opacity(0.65))
        .cornerRadius(10)
    }
}