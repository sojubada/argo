//
//  KeyboardHelpView.swift
//  Argo
//
//  ⌘/ 도움말. tvOS에는 메뉴 막대가 없어서 이 화면이 없으면
//  단축키가 있다는 사실조차 알 길이 없다. 반드시 있어야 한다.
//

import SwiftUI

@available(tvOS 13.0, *)
struct KeyboardHelpView: View {
    @Binding var isPresented: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                Image(systemName: "keyboard")
                Text("키보드 단축키")
                    .font(.title2)
                Spacer()
                Button(action: { isPresented = false }) {
                    Text(localizedString: "DISMISS")
                }
            }

            Text("키보드를 빼도 리모컨만으로 모든 기능이 됩니다. 단축키는 지름길입니다.")
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
                                HStack {
                                    Text(command.keyLabel)
                                        .font(.system(.body, design: .monospaced))
                                        .frame(width: 110, alignment: .leading)
                                    Text(command.title)
                                    Spacer()
                                }
                            }
                        }
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("스페이스로 고르기")
                            .font(.headline)
                            .foregroundColor(.accentColor)
                        Text("목록에서 스페이스를 누르면 그 줄이 골라지고 동그라미가 켜집니다. "
                             + "커서를 옮기며 여러 줄을 고를 수 있습니다. 다시 누르면 풀리고, "
                             + "마지막 하나를 풀면 동그라미가 사라집니다. 파일을 여는 것은 엔터입니다.")
                            .font(.caption)
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("글자 입력 점프")
                            .font(.headline)
                            .foregroundColor(.accentColor)
                        Text("목록에서 글자를 그대로 치면 그 글자로 시작하는 첫 항목으로 갑니다. 0.8초 쉬면 다시 처음부터 모읍니다.")
                            .font(.caption)
                    }
                }
                .padding(.vertical, 8)
            }
        }
        .padding(40)
    }
}
