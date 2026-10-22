//
//  FilePaneView.swift
//  Argo
//
//  목록을 그리는 뷰. FilePaneModel 하나만 받는다.
//  전역 상태를 참조하면 두 번 못 쓰기 때문이다.
//
//  파일을 여는 일은 하지 않는다. onOpen으로 ContentView에 돌려주고,
//  거기서 기존 defaultAction이 39개 시트를 그대로 띄운다.
//

import SwiftUI

/// 파일 종류 번호 → 아이콘. 원래 ContentView의 639줄 switch가 하던 일이다.
/// tvOS 13/14 분기로 같은 switch가 두 번 들어 있었다.
enum FileGlyph {
    static func symbol(for fileType: Double, name: String) -> String {
        switch Int(fileType) {
        case 0:  return name.hasSuffix("/") ? "folder" : "folder.fill"
        case 1:  return "waveform.circle"
        case 2:  return "video"
        case 3:  return "photo"
        case 4:  return "doc.text"
        case 6:  return "rectangle.compress.vertical"
        case 7:  return "terminal"
        case 8:  return "link"
        case 9:  return "archivebox"
        case 10: return "tray.full"
        case 11: return "textformat"
        case 12: return "photo.fill"
        case 13: return "externaldrive"
        case 14: return "safari"
        case 15: return "doc.richtext"
        case 16: return "doc.text.magnifyingglass"
        default: return "doc"
        }
    }
}

@available(tvOS 13.0, *)
struct FilePaneView: View {
    @ObservedObject var model: FilePaneModel

    /// 이 칸이 활성 칸인지. 테두리로 표시한다.
    let isActive: Bool

    /// 항목을 눌렀을 때. 인덱스는 model.masterFiles 기준이다.
    let onOpen: (Int) -> Void
    /// ".." 을 눌렀을 때.
    let onGoUp: () -> Void
    /// 이 칸에 포커스가 들어왔을 때. 활성 칸을 바꾸는 데 쓴다.
    let onFocus: () -> Void

    /// 길게 눌러 뜨는 메뉴. 활성 칸으로 바꾼 뒤 ContentView가 처리한다.
    let onContextMenu: ((Int) -> Void)?

    init(model: FilePaneModel,
         isActive: Bool,
         onOpen: @escaping (Int) -> Void,
         onGoUp: @escaping () -> Void,
         onFocus: @escaping () -> Void,
         onContextMenu: ((Int) -> Void)? = nil) {
        self.model = model
        self.isActive = isActive
        self.onOpen = onOpen
        self.onGoUp = onGoUp
        self.onFocus = onFocus
        self.onContextMenu = onContextMenu
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            list
        }
        .argoSurface()
        .overlay(
            // 활성 칸 표시. 두 칸이면 어느 쪽이 명령을 받는지 보여야 한다.
            RoundedRectangle(cornerRadius: Theme.corner)
                .stroke(isActive ? Theme.accent : Color.clear, lineWidth: 3)
        )
        // 칸을 하나의 포커스 구역으로 묶는다. 구역 안에서 더 갈 곳이 없으면
        // 포커스가 구역 밖으로 나가며, 좌우 끝에서는 툴바(위쪽 구역)로 빠진다.
        // tvOS 15부터 있는 수정자이고 배포 대상이 16이라 쓸 수 있다.
        //
        // 1픽셀 투명 뷰로 포커스를 '받아서' 돌려보내는 방식은 쓰지 않는다.
        // SwiftUI에는 포커스를 보낼 수단이 없어, 받은 뒤 갇힐 수 있다 —
        // 파일 탐색기에서 포커스가 갇히면 앱을 끄는 수밖에 없다.
        .focusSection()
    }

    // MARK: - 머리

    private var header: some View {
        VStack(spacing: 4) {
            pathRow
            if !model.filter.isEmpty {
                HStack(spacing: 6) {
                    Image(systemName: "line.3.horizontal.decrease.circle")
                    Text(model.filter)
                        .font(.caption)
                    Spacer()
                }
                .foregroundColor(Theme.accent)
                .padding(.horizontal, 12)
            }
        }
    }

    private var pathRow: some View {
        HStack(spacing: 12) {
            if let symbol = model.currentDirRisk.badgeSymbol {
                Image(systemName: symbol)
                    .foregroundColor(model.currentDirRisk.badgeColor)
            }
            // 긴 경로는 앞을 줄입니다 — 끝쪽(지금 폴더)이 중요합니다.
            Text(model.directory)
                .font(.caption)
                .foregroundColor(Theme.textPrimary)
                .lineLimit(1)
                .truncationMode(.head)
            Spacer()
            Text("\(model.masterFiles.count)")
                .font(.caption)
                .foregroundColor(Theme.textSecondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }

    // MARK: - 목록

    private var list: some View {
        List {
            Button(action: onGoUp) {
                HStack {
                    Image(systemName: "arrowshape.turn.up.left")
                    Text("..")
                }
            }

            ForEach(model.masterFiles.indices, id: \.self) { index in
                row(index)
            }
        }
    }

    @ViewBuilder
    private func row(_ index: Int) -> some View {
        // 행당 포커스 버튼은 하나다. tvOS 포커스 규칙이고, 깨뜨리면 목록을 못 쓴다.
        // #available은 컴파일 시점 검사라 .if 수정자로는 분기할 수 없어 통째로 두 벌이다.
        if #available(tvOS 14.0, *) {
            Button(action: {
                onFocus()
                onOpen(index)
            }) {
                rowContent(index)
            }
            .contextMenu {
                Button(action: {
                    onFocus()
                    onContextMenu?(index)
                }) {
                    Text("자세히")
                }
                // tvOS 컨텍스트 메뉴는 버튼을 눌러야 닫힌다. 다른 방법이 없다.
                Button("Dismiss", action: { })
            }
        } else {
            Button(action: {
                onFocus()
                onOpen(index)
            }) {
                rowContent(index)
            }
        }
    }

    @ViewBuilder
    private func rowContent(_ index: Int) -> some View {
        let file = model.masterFiles[index]
        HStack {
            if file.isLoadingFile {
                ProgressView()
            } else {
                if model.multiSelect {
                    Image(systemName: file.isSelected ? "checkmark.circle" : "circle")
                        .transition(.opacity)
                }

                // 위험도 배지. 부모 폴더가 안전하면 상속해 디스크를 덜 친다.
                let isSym = file.fileType == 8
                let itemRisk = PathSafety.fastRisk(parentRisk: model.currentDirRisk,
                                                   fullPath: file.fullPath,
                                                   isSymlink: isSym)
                if let symbol = itemRisk.badgeSymbol {
                    Image(systemName: symbol)
                        .foregroundColor(itemRisk.badgeColor)
                }

                Image(systemName: FileGlyph.symbol(for: file.fileType, name: file.name))

                // 심볼릭 링크는 이름 끝의 표시 문자를 떼고 보여준다.
                // 색을 못 박지 않는다. tvOS 는 포커스된 줄의 바탕을 흰색으로
                // 바꾸면서 글자색을 같이 뒤집는데, 여기서 흰색을 지정하면
                // 포커스된 줄의 파일명이 흰 바탕에 흰 글자가 되어 사라진다.
                Text(file.fileType == 8 ? removeLastChar(file.name) : file.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }
}
