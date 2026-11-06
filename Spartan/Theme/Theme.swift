//
//  Theme.swift
//  Argo
//
//  ArgoTheme(ObjC)의 SwiftUI 쪽 얼굴. 값은 저쪽 하나에서만 옵니다 —
//  두 군데 적으면 화면마다 다른 회색이 나옵니다.
//

import SwiftUI

@available(tvOS 13.0, *)
enum Theme {

    // MARK: - 색

    static var background: Color      { Color(ArgoTheme.background()) }
    static var canvasBackground: Color { Color(ArgoTheme.canvasBackground()) }
    static var surface: Color         { Color(ArgoTheme.surface()) }
    static var surfaceRaised: Color   { Color(ArgoTheme.surfaceRaised()) }

    static var textPrimary: Color     { Color(ArgoTheme.textPrimary()) }
    static var textSecondary: Color   { Color(ArgoTheme.textSecondary()) }
    static var textDisabled: Color    { Color(ArgoTheme.textDisabled()) }

    static var accent: Color          { Color(ArgoTheme.accent()) }
    static var airdropOn: Color       { Color(ArgoTheme.airdropOn()) }
    static var riskCaution: Color     { Color(ArgoTheme.riskCaution()) }
    static var riskForbidden: Color   { Color(ArgoTheme.riskForbidden()) }
    static var danger: Color          { Color(ArgoTheme.danger()) }

    // MARK: - 치수

    static var screenMargin: CGFloat  { ArgoTheme.screenMargin() }
    static var gutter: CGFloat        { ArgoTheme.gutter() }
    static var corner: CGFloat        { ArgoTheme.cornerRadius() }
    static var cornerSmall: CGFloat   { ArgoTheme.cornerRadiusSmall() }
}

/// 둥근 반투명 표면. 도구 모음과 패널의 바탕입니다.
@available(tvOS 13.0, *)
struct SurfaceBackground: ViewModifier {
    var radius: CGFloat = Theme.corner

    func body(content: Content) -> some View {
        content
            .background(Theme.surface)
            .cornerRadius(radius)
    }
}

@available(tvOS 13.0, *)
extension View {
    func argoSurface(radius: CGFloat = Theme.corner) -> some View {
        modifier(SurfaceBackground(radius: radius))
    }
}

// MARK: - 유리 섬 (Orbrisa 에서 배운 외관)
//
// 도구 모음을 화면 끝까지 늘어선 아이콘 줄 대신, 반투명 유리 '섬' 몇 개로
// 묶는다. 섬은 알약 모양이고, 흐린 바탕 위에 밝은 테두리 한 줄과 부드러운
// 그림자를 둔다. 단추는 평소 옅은 흰 원, 포커스가 오면 강조색으로 채우고
// 조금 커진다 — 어느 단추에 있는지 멀리서도 보이게.
//
// 치수는 여기 한 곳에만 둔다. 화면마다 50, 56, 60 이 섞여 있던 것을 모았다.

@available(tvOS 15.0, *)
extension Theme {
    /// 도구 모음 단추 지름
    static var barButton: CGFloat { 56 }
    /// 포커스된 단추 배율 (tvOS 카드 효과 1.1 보다 살짝 크게, 동그라미라 작아 보여서)
    static var focusScale: CGFloat { 1.12 }

    /// 유리 효과 켜짐(설정). 끄면 같은 색의 불투명 바탕을 쓴다.
    static var glassEnabled: Bool {
        UserDefaults.settings.object(forKey: "argoGlass") as? Bool ?? true
    }
}

/// 유리 섬 바탕.
@available(tvOS 15.0, *)
struct GlassIsland: ViewModifier {
    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                ZStack {
                    if Theme.glassEnabled {
                        Capsule().fill(.ultraThinMaterial)
                        Capsule().fill(Theme.surface.opacity(0.35))
                    } else {
                        Capsule().fill(Theme.surfaceRaised)
                    }
                    // 밝은 테두리 한 줄이 유리의 가장자리를 만든다.
                    Capsule().strokeBorder(Color.white.opacity(0.18), lineWidth: 1)
                }
                .shadow(color: .black.opacity(0.30), radius: 22, x: 0, y: 10)
            )
    }
}

/// 섬 안의 둥근 단추. 아이콘 하나면 원, 글자가 붙으면 알약.
@available(tvOS 15.0, *)
struct IslandButtonStyle: ButtonStyle {
    var prominent = false
    var destructive = false

    func makeBody(configuration: Configuration) -> some View {
        IslandButtonBody(configuration: configuration, prominent: prominent, destructive: destructive)
    }
}

@available(tvOS 15.0, *)
private struct IslandButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let prominent: Bool
    let destructive: Bool
    @Environment(\.isFocused) private var focused
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        let fill: Color = focused
            ? (destructive ? Theme.danger : Theme.accent)
            : (prominent ? Theme.accent.opacity(0.85) : Color.white.opacity(0.12))
        let ink: Color = (focused || prominent) ? Theme.background : Theme.textPrimary
        configuration.label
            .font(.system(size: 26, weight: .semibold))
            .foregroundColor(enabled ? ink : Theme.textDisabled)
            .frame(minWidth: Theme.barButton, minHeight: Theme.barButton)
            .padding(.horizontal, 2)
            .background(Capsule().fill(fill))
            .scaleEffect(configuration.isPressed ? 0.96 : (focused ? Theme.focusScale : 1))
            .shadow(color: .black.opacity(focused ? 0.45 : 0), radius: focused ? 14 : 0, x: 0, y: 8)
            .animation(.easeOut(duration: 0.15), value: focused)
            // 단추 사이 간격. 막대 쪽 HStack 은 간격 0 이다(접힌 단추가 빈 칸을 남기지 않게).
            .padding(.horizontal, 7)
    }
}

@available(tvOS 15.0, *)
extension View {
    func glassIsland() -> some View { modifier(GlassIsland()) }
}
