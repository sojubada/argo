//
//  AddToFavoritesView.swift
//  Argo
//

import SwiftUI

struct AddToFavoritesView: View {
    @State var filePath: String
    @State var displayName: String
    @Binding var showView: Bool

    @State private var selectedIcon: String = "folder"

    var body: some View {
        VStack(spacing: 24) {
            Text(NSLocalizedString("FAVORITESADD", comment: ""))
                .font(.title2)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }

            ArgoTextField(NSLocalizedString("FAVORITESADD_DISPLAYNAME", comment: ""), text: $displayName)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }

            ArgoTextField(NSLocalizedString("FAVORITESADD_FILEPATH", comment: ""), text: $filePath, onEditingChanged: { isEditing in
                if !isEditing {
                    filePath = FavoritesStore.normalizePath(filePath)
                }
            })
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }

            VStack(spacing: 12) {
                Text("Icon")
                    .font(.caption)
                    .foregroundColor(.secondary)

                HStack(spacing: 16) {
                    ForEach(FavoritesStore.availableIcons, id: \.self) { iconName in
                        Button(action: {
                            selectedIcon = iconName
                        }) {
                            VStack(spacing: 6) {
                                Image(systemName: iconName)
                                    .font(.title3)
                                    .frame(width: 50, height: 50)
                                Image(systemName: selectedIcon == iconName ? "checkmark.circle.fill" : "circle")
                                    .font(.caption2)
                                    .foregroundColor(selectedIcon == iconName ? .primary : .secondary)
                            }
                        }
                    }
                }
            }

            HStack(spacing: 24) {
                Button(action: {
                    let normalizedPath = FavoritesStore.normalizePath(filePath)
                    filePath = normalizedPath
                    FavoritesStore.shared.add(
                        name: displayName,
                        path: normalizedPath,
                        systemImage: selectedIcon
                    )
                    showView = false
                }) {
                    Text(NSLocalizedString("CONFIRM", comment: ""))
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                        }
                }
                .disabled(filePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                Button(action: {
                    showView = false
                }) {
                    Text("Cancel")
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                        }
                }
            }
        }
        .padding(40)
        .onAppear {
            filePath = FavoritesStore.normalizePath(filePath)
            if displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let last = URL(fileURLWithPath: filePath).lastPathComponent
                displayName = (last.isEmpty || last == "/") ? "Root" : last
            }
        }
    }
}