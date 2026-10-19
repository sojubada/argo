//
//  PDFReaderView.swift
//  Argo
//

import SwiftUI
import PDFKit

private enum PDFActiveSheet: Identifiable {
    case outlineAndBookmarks
    case thumbnails
    case displaySettings
    case themeSettings

    var id: Int {
        switch self {
        case .outlineAndBookmarks: return 1
        case .thumbnails: return 2
        case .displaySettings: return 3
        case .themeSettings: return 4
        }
    }
}

struct PDFReaderView: View {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    @StateObject private var model: PDFReaderModel

    @State private var showTopToolbar: Bool = false
    @State private var showBottomBar: Bool = false
    @State private var toolbarAutoHideTask: DispatchWorkItem? = nil

    @State private var passwordInput: String = ""
    @State private var passwordError: String = ""

    // tvOS 단일 .sheet 제약 준수
    @State private var activeSheet: PDFActiveSheet? = nil

    @FocusState private var canvasFocused: Bool

    init(filePath: String, fileName: String, isPresented: Binding<Bool>) {
        self.filePath = filePath
        self.fileName = fileName
        self._isPresented = isPresented
        self._model = StateObject(wrappedValue: PDFReaderModel(filePath: filePath, fileName: fileName))
    }

    var body: some View {
        ZStack {
            Color.black.edgesIgnoringSafeArea(.all)

            if let err = model.loadError {
                errorView(message: err)
            } else if model.isLocked {
                lockedPasswordView
            } else {
                readerMainBody
            }
        }
        .sheet(item: $activeSheet) { sheet in
            sheetContent(for: sheet)
        }
        .onExitCommand {
            handleExitCommand()
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

            Button(action: {
                isPresented = false
            }) {
                Text("Close")
            }
        }
        .padding(40)
    }

    private var lockedPasswordView: some View {
        VStack(spacing: 24) {
            Image(systemName: "lock.doc.fill")
                .font(.largeTitle)

            Text("This PDF is Password Protected")
                .font(.title2)

            Text(fileName)
                .font(.caption)
                .foregroundColor(.secondary)

            SecureField("Password", text: $passwordInput)
                .frame(maxWidth: 600)

            if !passwordError.isEmpty {
                Text(passwordError)
                    .font(.caption)
                    .foregroundColor(.red)
            }

            HStack(spacing: 24) {
                Button(action: {
                    if model.unlock(password: passwordInput) {
                        passwordError = ""
                        passwordInput = ""
                    } else {
                        passwordError = "Incorrect password. Please try again."
                    }
                }) {
                    Text("Unlock")
                }

                Button(action: {
                    isPresented = false
                }) {
                    Text("Cancel")
                }
            }
        }
        .padding(40)
    }

    // MARK: - Main Reader Body

    private var readerMainBody: some View {
        ZStack {
            // PDF 렌더링 컨테이너 + 테마 및 밝기 적용
            themedPDFContainer
                .edgesIgnoringSafeArea(.all)

            // 읽기 모드 / 확대 모드에서 리모컨 방향키·클릭·재생 버튼을 받는 투명 포커스 영역
            if !showTopToolbar && !showBottomBar && activeSheet == nil {
                remoteControlCatcher
            }

            // 상단 툴바 & 하단 진행바 오버레이 (5% 오버스캔 안전 여백 확보)
            VStack {
                if showTopToolbar {
                    topToolbarView
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
                        .padding(.bottom, 16)
                }

                if model.interactionMode == .zoomPan && !showTopToolbar && !showBottomBar {
                    Text("Zoom Mode (\(model.zoomStepLabel)) · Swipe to Pan · Select to Cycle Zoom · Menu to Return")
                        .font(.caption)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 10)
                        .background(Color.black.opacity(0.75))
                        .cornerRadius(10)
                        .padding(.bottom, 16)
                }

                if showBottomBar {
                    bottomProgressOverlay
                        .transition(.opacity)
                } else {
                    compactBottomStatus
                }
            }
            .padding(48) // TV 화면 가장자리 5% 오버스캔 영역 보호
        }
    }

    @ViewBuilder
    private var themedPDFContainer: some View {
        switch model.theme {
        case .original:
            PDFKitContainer(model: model)
                .brightness(model.brightness - 1.0)
        case .night:
            PDFKitContainer(model: model)
                .colorInvert()
                .hueRotation(.degrees(180))
                .brightness(model.brightness - 1.0)
        case .sepia:
            PDFKitContainer(model: model)
                .colorMultiply(Color(red: 0.98, green: 0.94, blue: 0.84))
                .brightness(model.brightness - 1.0)
        }
    }

    private var remoteControlCatcher: some View {
        Button(action: {
            if model.interactionMode == .zoomPan {
                model.cycleZoomStep()
            } else {
                showToolbarWithAutoHide()
            }
        }) {
            Color.clear
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focused($canvasFocused)
        .onMoveCommand { direction in
            handleMoveCommand(direction)
        }
        .onPlayPauseCommand {
            model.toggleAutoAdvance()
        }
        .onAppear {
            canvasFocused = true
        }
    }

    private func handleMoveCommand(_ direction: MoveCommandDirection) {
        if model.interactionMode == .zoomPan {
            let step: CGFloat = 180
            switch direction {
            case .left:
                model.pan(dx: -step, dy: 0)
            case .right:
                model.pan(dx: step, dy: 0)
            case .up:
                model.pan(dx: 0, dy: -step)
            case .down:
                model.pan(dx: 0, dy: step)
            @unknown default:
                break
            }
        } else {
            switch direction {
            case .left:
                if model.displayModeOption == .continuous {
                    model.pan(dx: 0, dy: -320)
                } else {
                    model.previousPage()
                }
            case .right:
                if model.displayModeOption == .continuous {
                    model.pan(dx: 0, dy: 320)
                } else {
                    model.nextPage()
                }
            case .up:
                showToolbarWithAutoHide()
            case .down:
                withAnimation {
                    showTopToolbar = false
                    showBottomBar = true
                }
            @unknown default:
                break
            }
        }
    }

    private func showToolbarWithAutoHide() {
        withAnimation {
            showBottomBar = false
            showTopToolbar = true
        }
        scheduleToolbarAutoHide()
    }

    private func scheduleToolbarAutoHide() {
        toolbarAutoHideTask?.cancel()
        let task = DispatchWorkItem {
            if activeSheet == nil {
                withAnimation {
                    showTopToolbar = false
                }
            }
        }
        toolbarAutoHideTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 5.0, execute: task)
    }

    // MARK: - Top Toolbar

    private var topToolbarView: some View {
        VStack(spacing: 10) {
            Text(fileName)
                .font(.headline)
                .lineLimit(1)

            HStack(spacing: 16) {
                // 이전 페이지
                Button(action: {
                    scheduleToolbarAutoHide()
                    model.previousPage()
                }) {
                    Image(systemName: "chevron.left")
                        .frame(width: 48, height: 48)
                }
                .disabled(model.currentPageIndex <= 0)

                // 다음 페이지
                Button(action: {
                    scheduleToolbarAutoHide()
                    model.nextPage()
                }) {
                    Image(systemName: "chevron.right")
                        .frame(width: 48, height: 48)
                }
                .disabled(model.currentPageIndex >= model.totalPages - 1)

                // 목차 & 북마크
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    activeSheet = .outlineAndBookmarks
                }) {
                    Image(systemName: "list.bullet.indent")
                        .frame(width: 48, height: 48)
                }
                .disabled(model.outlineEntries.isEmpty && model.bookmarks.isEmpty)

                // 썸네일 격자
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    activeSheet = .thumbnails
                }) {
                    Image(systemName: "square.grid.2x2")
                        .frame(width: 48, height: 48)
                }

                // 보기 모드 전환
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    activeSheet = .displaySettings
                }) {
                    Image(systemName: "textformat.size")
                        .frame(width: 48, height: 48)
                }

                // 테마 & 밝기 전환
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    activeSheet = .themeSettings
                }) {
                    Image(systemName: "sun.max")
                        .frame(width: 48, height: 48)
                }

                // 확대/패닝 모드 진입
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    withAnimation {
                        showTopToolbar = false
                        showBottomBar = false
                    }
                    if model.interactionMode == .zoomPan {
                        model.interactionMode = .reading
                        model.zoomStepIndex = 0
                    } else {
                        model.interactionMode = .zoomPan
                        model.zoomStepIndex = 2 // 기본 150% 확대 진입
                    }
                    model.applyScaleToPDFView()
                }) {
                    Image(systemName: model.interactionMode == .zoomPan ? "arrow.down.right.and.arrow.up.left" : "plus.magnifyingglass")
                        .frame(width: 48, height: 48)
                }

                // 현재 페이지 북마크 토글
                Button(action: {
                    scheduleToolbarAutoHide()
                    model.toggleBookmarkForCurrentPage()
                }) {
                    Image(systemName: model.isCurrentPageBookmarked ? "bookmark.fill" : "bookmark")
                        .frame(width: 48, height: 48)
                }

                // 닫기
                Button(action: {
                    toolbarAutoHideTask?.cancel()
                    isPresented = false
                }) {
                    Image(systemName: "xmark")
                        .frame(width: 48, height: 48)
                }
            }
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 16)
        .background(Color.black.opacity(0.82))
        .cornerRadius(16)
    }

    // MARK: - Bottom Progress Bar & Page Jumper

    private var compactBottomStatus: some View {
        HStack(spacing: 16) {
            Text("\(model.currentPageIndex + 1) / \(max(1, model.totalPages))")
                .font(.caption)
            ProgressView(value: model.progressFraction)
                .frame(width: 240)
            Text("\(model.remainingPercentageInt)% left")
                .font(.caption)
                .foregroundColor(.secondary)
            if model.isCurrentPageBookmarked {
                Image(systemName: "bookmark.fill")
                    .font(.caption)
            }
            if model.autoAdvanceEnabled {
                Text("Auto")
                    .font(.caption)
                    .foregroundColor(.green)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 8)
        .background(Color.black.opacity(0.65))
        .cornerRadius(10)
    }

    private var bottomProgressOverlay: some View {
        VStack(spacing: 14) {
            HStack(spacing: 20) {
                Text("Page \(model.currentPageIndex + 1) / \(max(1, model.totalPages))")
                    .font(.headline)

                ProgressView(value: model.progressFraction)
                    .frame(width: 400)

                Text("\(model.remainingPercentageInt)% remaining")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack(spacing: 18) {
                Button(action: { model.goToPage(0) }) {
                    Text("First")
                }
                .disabled(model.currentPageIndex <= 0)

                Button(action: { model.goToPage(model.currentPageIndex - 10) }) {
                    Text("-10")
                }
                .disabled(model.currentPageIndex <= 0)

                Button(action: { model.previousPage() }) {
                    Text("-1")
                }
                .disabled(model.currentPageIndex <= 0)

                Button(action: { model.nextPage() }) {
                    Text("+1")
                }
                .disabled(model.currentPageIndex >= model.totalPages - 1)

                Button(action: { model.goToPage(model.currentPageIndex + 10) }) {
                    Text("+10")
                }
                .disabled(model.currentPageIndex >= model.totalPages - 1)

                Button(action: { model.goToPage(model.totalPages - 1) }) {
                    Text("Last")
                }
                .disabled(model.currentPageIndex >= model.totalPages - 1)

                Button(action: {
                    withAnimation {
                        showBottomBar = false
                    }
                }) {
                    Text("Done")
                }
            }
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 18)
        .background(Color.black.opacity(0.85))
        .cornerRadius(16)
    }

    // MARK: - Single Sheet Content Switcher

    @ViewBuilder
    private func sheetContent(for sheet: PDFActiveSheet) -> some View {
        switch sheet {
        case .outlineAndBookmarks:
            PDFOutlineView(
                model: model,
                onSelectPage: { pageIdx in
                    model.goToPage(pageIdx)
                    activeSheet = nil
                },
                onClose: {
                    activeSheet = nil
                }
            )
        case .thumbnails:
            PDFThumbnailGridView(
                model: model,
                onSelectPage: { pageIdx in
                    model.goToPage(pageIdx)
                    activeSheet = nil
                },
                onClose: {
                    activeSheet = nil
                }
            )
        case .displaySettings:
            displaySettingsPanel
        case .themeSettings:
            themeSettingsPanel
        }
    }

    private var displaySettingsPanel: some View {
        VStack(spacing: 24) {
            Text("Display Mode & Scaling")
                .font(.title2)

            VStack(spacing: 12) {
                Text("Page Layout")
                    .font(.caption)
                    .foregroundColor(.secondary)

                HStack(spacing: 20) {
                    ForEach(PDFDisplayModeOption.allCases) { option in
                        Button(action: {
                            model.displayModeOption = option
                            model.savePreferences()
                        }) {
                            HStack(spacing: 8) {
                                Image(systemName: model.displayModeOption == option ? "checkmark.circle.fill" : "circle")
                                Text(option.rawValue)
                            }
                        }
                    }
                }
            }

            VStack(spacing: 12) {
                Text("Page Scaling")
                    .font(.caption)
                    .foregroundColor(.secondary)

                HStack(spacing: 20) {
                    ForEach(PDFFitPolicy.allCases) { policy in
                        Button(action: {
                            model.fitPolicy = policy
                            model.savePreferences()
                            model.applyScaleToPDFView()
                        }) {
                            HStack(spacing: 8) {
                                Image(systemName: model.fitPolicy == policy ? "checkmark.circle.fill" : "circle")
                                Text(policy.rawValue)
                            }
                        }
                    }
                }
            }

            Button(action: {
                activeSheet = nil
            }) {
                Text("Done")
            }
        }
        .padding(40)
    }

    private var themeSettingsPanel: some View {
        VStack(spacing: 24) {
            Text("Reading Theme & Brightness")
                .font(.title2)

            VStack(spacing: 12) {
                Text("Color Theme")
                    .font(.caption)
                    .foregroundColor(.secondary)

                HStack(spacing: 20) {
                    ForEach(PDFThemeOption.allCases) { option in
                        Button(action: {
                            model.theme = option
                            model.savePreferences()
                        }) {
                            HStack(spacing: 8) {
                                Image(systemName: model.theme == option ? "checkmark.circle.fill" : "circle")
                                Text(option.rawValue)
                            }
                        }
                    }
                }
            }

            VStack(spacing: 12) {
                Text("Brightness: \(Int(round(model.brightness * 100)))%")
                    .font(.headline)

                HStack(spacing: 20) {
                    Button(action: {
                        model.brightness = max(0.3, (model.brightness - 0.1))
                        model.savePreferences()
                    }) {
                        Text("- 10%")
                    }
                    .disabled(model.brightness <= 0.31)

                    ProgressView(value: (model.brightness - 0.3) / 0.7)
                        .frame(width: 300)

                    Button(action: {
                        model.brightness = min(1.0, (model.brightness + 0.1))
                        model.savePreferences()
                    }) {
                        Text("+ 10%")
                    }
                    .disabled(model.brightness >= 0.99)

                    Button(action: {
                        model.brightness = 1.0
                        model.savePreferences()
                    }) {
                        Text("Reset (100%)")
                    }
                }
            }

            Button(action: {
                activeSheet = nil
            }) {
                Text("Done")
            }
        }
        .padding(40)
    }

    // MARK: - Exit Command (Menu Button Step-by-Step Back)

    private func handleExitCommand() {
        if activeSheet != nil {
            activeSheet = nil
        } else if showTopToolbar {
            toolbarAutoHideTask?.cancel()
            withAnimation {
                showTopToolbar = false
            }
        } else if showBottomBar {
            withAnimation {
                showBottomBar = false
            }
        } else if model.interactionMode == .zoomPan {
            model.interactionMode = .reading
            model.zoomStepIndex = 0
            model.applyScaleToPDFView()
        } else {
            toolbarAutoHideTask?.cancel()
            isPresented = false
        }
    }
}