//
//  ArgoTheme.h
//  Argo
//
//  색과 치수의 단일 출처. 뷰어와 탐색기가 같은 것을 읽습니다.
//  지금까지는 파일마다 colorWithWhite:0.07 같은 숫자를 따로 적어
//  화면마다 조금씩 다른 회색이 나왔습니다.
//
//  거실 기준으로 잡았습니다 — 3미터 떨어져 보는 화면입니다.
//  글자는 크고, 대비는 확실하고, 포인트 색은 하나입니다.
//

#import <UIKit/UIKit.h>

@interface ArgoTheme : NSObject

#pragma mark - 색

/// 화면 바탕. 순검정이 아닌 짙은 차콜 — 순검정은 OLED에서 경계가 사라집니다.
+ (UIColor *)background;
/// 문서가 놓이는 바탕. 주간에는 흰 종이가 떠 보이도록 약간 밝습니다.
+ (UIColor *)canvasBackground;
/// 도구 모음·패널의 반투명 표면.
+ (UIColor *)surface;
/// 표면 위에 올라가는 한 단 밝은 면 (버튼 바탕).
+ (UIColor *)surfaceRaised;
/// 포커스를 받은 컨트롤의 바탕.
+ (UIColor *)surfaceFocused;

/// 본문 텍스트.
+ (UIColor *)textPrimary;
/// 보조 텍스트 — 쪽 번호, 설명.
+ (UIColor *)textSecondary;
/// 비활성.
+ (UIColor *)textDisabled;

/// 포인트 색. 하나만 씁니다.
+ (UIColor *)accent;

/// AirDrop 이 켜져 있을 때의 파랑. 포인트 색(청록)과 구별되어야 합니다 —
/// 켜짐은 상태이지 강조가 아닙니다.
+ (UIColor *)airdropOn;

/// 위험도 — 탐색기의 배지와 같은 값을 씁니다.
+ (UIColor *)riskCaution;
+ (UIColor *)riskForbidden;
+ (UIColor *)danger;

#pragma mark - 치수

/// 화면 테두리 여백. TV는 가장자리가 잘릴 수 있어 넉넉히 둡니다.
+ (CGFloat)screenMargin;
+ (CGFloat)gutter;
+ (CGFloat)cornerRadius;
+ (CGFloat)cornerRadiusSmall;

#pragma mark - 글자

+ (UIFont *)fontTitle;
+ (UIFont *)fontBody;
+ (UIFont *)fontCaption;
/// 쪽 번호처럼 숫자가 바뀌어도 폭이 안 흔들려야 하는 곳.
+ (UIFont *)fontMonoBody;

@end


/// 포커스가 눈에 확실히 보이는 알약 버튼.
/// tvOS 기본 시스템 버튼은 포커스 표시가 거의 없어 리모컨으로 쓰기 어렵습니다.
/// 과한 발광이나 큰 확대 대신 부드러운 크기 변화와 명암 대비로 알립니다.
@interface ArgoBarButton : UIButton
/// 포인트 색으로 칠해 주 동작임을 보입니다. 화면에 하나만 두십시오.
@property (nonatomic, assign) BOOL isPrimary;
/// nonnull 로 둡니다 — 아니면 Swift 에서 선택형으로 들어와 전부 풀어야 합니다.
/// NS_SWIFT_NAME 으로 Swift 쪽 이름을 못 박습니다. 자동 변환에 맡기면
/// 클래스 이름과 겹치는 부분을 깎아내 예측하기 어려운 이름이 됩니다.
+ (nonnull instancetype)barButtonWithTitle:(nonnull NSString *)title
    NS_SWIFT_NAME(make(title:));
@end


/// 모서리가 둥근 반투명 표면. 도구 모음과 패널의 바탕입니다.
@interface ArgoSurfaceView : UIView
+ (nonnull instancetype)makeSurface;
@end
