//
//  KeyboardHelpView.swift
//  Argo
//
//  ⌘/ 도움말. tvOS에는 메뉴 막대가 없어서 이 화면이 없으면
//  단축키가 있다는 사실조차 알 길이 없다. 반드시 있어야 한다.
//

import SwiftUI

/// 단축키 한 줄.
///
/// 포커스를 받습니다. tvOS 의 ScrollView 는 포커스가 아래로 내려갈 때
/// 따라 움직이는 물건이라, 안에 포커스 받을 것이 하나도 없으면 아무리
/// 쓸어내려도 꼼짝하지 않습니다. 전에는 줄이 전부 Text 였고 포커스를
/// 받는 것은 ScrollView 밖의 닫기 단추뿐이어서, 첫 화면에 들어온 만큼만
/// 보이고 나머지는 볼 길이 없었습니다.
@available(tvOS 13.0, *)
private struct HelpRow: View {
    let key: String
    let title: String
    @FocusState private var focused: Bool

    var body: some View {
        HStack {
            Text(key)
                .font(.system(.body, design: .monospaced))
                .frame(width: 110, alignment: .leading)
            Text(title)
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(focused ? Color.primary.opacity(0.18) : Color.clear)
        )
        .focusable()
        .focused($focused)
    }
}

@available(tvOS 13.0, *)
struct KeyboardHelpView: View {
    @Binding var isPresented: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                Image(systemName: "keyboard")
                Text(L("키보드 단축키"))
                    .font(.title2)
                Spacer()
                Button(action: { isPresented = false }) {
                    Text(localizedString: "DISMISS")
                }
            }

            Text(L("키보드를 빼도 리모컨만으로 모든 기능이 됩니다. 단축키는 지름길입니다."))
                .font(.caption)
                .foregroundColor(.secondary)

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    ForEach(ArgoCommand.groups.indices, id: \.self) { i in
                        let group = ArgoCommand.groups[i]
                        VStack(alignment: .leading, spacing: 8) {
                            Text(group.0)
                                .font(.headline)
                                .foregroundColor(.accentColor)
                            ForEach(group.1, id: \.rawValue) { command in
                                HelpRow(key: command.keyLabel, title: command.title)
                            }
                        }
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text(L("스페이스로 고르기"))
                            .font(.headline)
                            .foregroundColor(.accentColor)
                        Text(L("목록에서 스페이스를 누르면 그 줄이 골라지고 동그라미가 켜집니다. ")
                             + L("커서를 옮기며 여러 줄을 고를 수 있습니다. 다시 누르면 풀리고, ")
                             + L("마지막 하나를 풀면 동그라미가 사라집니다. 파일을 여는 것은 엔터입니다."))
                            .font(.caption)
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text(L("글자 입력 점프"))
                            .font(.headline)
                            .foregroundColor(.accentColor)
                        Text(L("목록에서 글자를 그대로 치면 그 글자로 시작하는 첫 항목으로 갑니다. 0.8초 쉬면 다시 처음부터 모읍니다."))
                            .font(.caption)
                    }
                }
                .padding(.vertical, 8)
            }
        }
        .padding(40)
    }
}
