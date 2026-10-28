//
//  FavoritesView.swift
//  Argo
//

import SwiftUI

struct FavoritesView: View {
    @Binding var directory: String
    @Binding var showView: Bool

    @ObservedObject private var store = FavoritesStore.shared

    @State private var editMode = false
    @State private var editingItem: FavoriteItem? = nil
    @State private var renameText = ""
    @State private var addToFavoritesShow = false

    private var isCurrentDirectoryAdded: Bool {
        store.contains(path: directory)
    }

    var body: some View {
        VStack(spacing: 16) {
            Text(NSLocalizedString("FAVORITES_TITLE", comment: ""))
                .font(.title2)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }

            if let item = editingItem {
                editPanel(for: item)
            } else {
                listPanel
            }
        }
        .padding(40)
        .onExitCommand {
            handleExit()
        }
        .sheet(isPresented: $addToFavoritesShow) {
            AddToFavoritesView(
                filePath: directory,
                displayName: defaultFolderName(for: directory),
                showView: $addToFavoritesShow
            )
        }
    }

    // MARK: - List Panel

    private var listPanel: some View {
        VStack(spacing: 16) {
            if store.items.isEmpty {
                Spacer()
                Text("No favorites yet.")
                    .font(.headline)
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                // 행당 버튼 하나 규칙 준수
                List(store.items) { item in
                    Button(action: {
                        if editMode {
                            editingItem = item
                            renameText = item.name
                        } else {
                            directory = FavoritesStore.normalizePath(item.path)
                            showView = false
                        }
                    }) {
                        HStack(spacing: 16) {
                            Image(systemName: editMode ? "pencil.circle" : item.systemImage)
                                .frame(width: 36)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(item.name)
                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                    }
                                Text(item.path)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }

            Text(editMode
                 ? "Edit mode: choose a favorite to rename, reorder, or delete."
                 : "Choose a favorite to jump to that folder.")
                .font(.caption)
                .foregroundColor(.secondary)

            HStack(spacing: 20) {
                // 현재 폴더 한 번에 추가
                Button(action: {
                    let name = defaultFolderName(for: directory)
                    store.add(name: name, path: directory, systemImage: "folder")
                }) {
                    HStack {
                        Image(systemName: isCurrentDirectoryAdded ? "checkmark.circle" : "plus.circle")
                        Text(isCurrentDirectoryAdded ? "Already Added" : "Add Current Folder")
                    }
                }
                .disabled(isCurrentDirectoryAdded)

                // 상세 설정(이름/경로/아이콘 지정)으로 추가
                Button(action: {
                    addToFavoritesShow = true
                }) {
                    HStack {
                        Image(systemName: "plus")
                        Text(NSLocalizedString("FAVORITESADD", comment: ""))
                    }
                }

                // 편집 모드 토글
                if !store.items.isEmpty {
                    Button(action: {
                        editMode.toggle()
                    }) {
                        HStack {
                            Image(systemName: "gearshape")
                            Text(editMode ? "Done" : "Edit")
                        }
                    }
                }

                // 닫기
                Button(action: {
                    showView = false
                }) {
                    HStack {
                        Image(systemName: "xmark")
                        Text("Close")
                    }
                }
            }
        }
    }

    // MARK: - Edit Panel

    private func editPanel(for item: FavoriteItem) -> some View {
        let currentIndex = store.items.firstIndex(where: { $0.id == item.id })
        let canMoveUp = (currentIndex ?? 0) > 0
        let canMoveDown = (currentIndex ?? store.items.count) < (store.items.count - 1)

        return VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 12) {
                Image(systemName: item.systemImage)
                    .font(.title2)
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.name)
                        .font(.headline)
                    Text(item.path)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            ArgoTextField(NSLocalizedString("FAVORITESADD_DISPLAYNAME", comment: ""), text: $renameText)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }

            HStack(spacing: 20) {
                Button(action: {
                    store.rename(item, to: renameText)
                    if let updated = store.items.first(where: { $0.id == item.id }) {
                        editingItem = updated
                    }
                }) {
                    HStack {
                        Image(systemName: "checkmark")
                        Text("Rename")
                    }
                }
                .disabled(renameText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                Button(action: {
                    store.moveUp(item)
                    if let updated = store.items.first(where: { $0.id == item.id }) {
                        editingItem = updated
                    }
                }) {
                    HStack {
                        Image(systemName: "arrow.up")
                        Text("Move Up")
                    }
                }
                .disabled(!canMoveUp)

                Button(action: {
                    store.moveDown(item)
                    if let updated = store.items.first(where: { $0.id == item.id }) {
                        editingItem = updated
                    }
                }) {
                    HStack {
                        Image(systemName: "arrow.down")
                        Text("Move Down")
                    }
                }
                .disabled(!canMoveDown)

                Button(action: {
                    store.remove(item)
                    editingItem = nil
                    if store.items.isEmpty {
                        editMode = false
                    }
                }) {
                    HStack {
                        Image(systemName: "trash")
                        Text("Delete")
                    }
                    .foregroundColor(.red)
                }

                Button(action: {
                    editingItem = nil
                }) {
                    Text("Back")
                }
            }

            Spacer()
        }
        .padding()
    }

    // MARK: - Helpers

    private func defaultFolderName(for path: String) -> String {
        let name = URL(fileURLWithPath: path).lastPathComponent
        if name.isEmpty || name == "/" {
            return "Root"
        }
        return name
    }

    private func handleExit() {
        if editingItem != nil {
            editingItem = nil
        } else if editMode {
            editMode = false
        } else {
            showView = false
        }
    }
}