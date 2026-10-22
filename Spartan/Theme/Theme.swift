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
