//
//  ArgoTheme.m
//  Argo
//

#import "ArgoTheme.h"

@implementation ArgoTheme

#pragma mark - 색

+ (UIColor *)background      { return [UIColor colorWithRed:0.07 green:0.07 blue:0.08 alpha:1.0]; }
+ (UIColor *)canvasBackground { return [UIColor colorWithRed:0.13 green:0.13 blue:0.15 alpha:1.0]; }
+ (UIColor *)surface         { return [UIColor colorWithRed:0.16 green:0.16 blue:0.18 alpha:0.92]; }
+ (UIColor *)surfaceRaised   { return [UIColor colorWithRed:0.24 green:0.24 blue:0.27 alpha:1.0]; }
+ (UIColor *)surfaceFocused  { return [UIColor colorWithRed:0.95 green:0.95 blue:0.96 alpha:1.0]; }

+ (UIColor *)textPrimary     { return [UIColor colorWithWhite:0.97 alpha:1.0]; }
+ (UIColor *)textSecondary   { return [UIColor colorWithWhite:0.66 alpha:1.0]; }
+ (UIColor *)textDisabled    { return [UIColor colorWithWhite:0.38 alpha:1.0]; }

/// 차분한 청록. 애플TV의 짙은 바탕에서 초록·파랑보다 눈이 덜 피곤합니다.
+ (UIColor *)accent          { return [UIColor colorWithRed:0.29 green:0.69 blue:0.78 alpha:1.0]; }
+ (UIColor *)airdropOn       { return [UIColor colorWithRed:0.20 green:0.52 blue:0.98 alpha:1.0]; }

+ (UIColor *)riskCaution     { return [UIColor colorWithRed:0.95 green:0.73 blue:0.25 alpha:1.0]; }
+ (UIColor *)riskForbidden   { return [UIColor colorWithRed:0.90 green:0.35 blue:0.32 alpha:1.0]; }
+ (UIColor *)danger          { return [UIColor colorWithRed:0.90 green:0.35 blue:0.32 alpha:1.0]; }

#pragma mark - 치수

+ (CGFloat)screenMargin     { return 60.0; }
+ (CGFloat)gutter           { return 24.0; }
+ (CGFloat)cornerRadius     { return 18.0; }
+ (CGFloat)cornerRadiusSmall { return 10.0; }

#pragma mark - 글자

+ (UIFont *)fontTitle   { return [UIFont systemFontOfSize:34 weight:UIFontWeightSemibold]; }
+ (UIFont *)fontBody    { return [UIFont systemFontOfSize:28 weight:UIFontWeightMedium]; }
+ (UIFont *)fontCaption { return [UIFont systemFontOfSize:24 weight:UIFontWeightRegular]; }
+ (UIFont *)fontMonoBody {
    return [UIFont monospacedDigitSystemFontOfSize:28 weight:UIFontWeightMedium];
}

@end


#pragma mark - 알약 버튼

@implementation ArgoBarButton

+ (instancetype)barButtonWithTitle:(NSString *)title {
    ArgoBarButton *b = [ArgoBarButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:title forState:UIControlStateNormal];
    b.titleLabel.font = [ArgoTheme fontBody];
    b.contentEdgeInsets = UIEdgeInsetsMake(14, 28, 14, 28);
    b.layer.cornerRadius = [ArgoTheme cornerRadiusSmall];
    b.layer.masksToBounds = YES;
    [b applyUnfocusedStyle];
    return b;
}

- (void)setIsPrimary:(BOOL)isPrimary {
    _isPrimary = isPrimary;
    [self applyUnfocusedStyle];
}

- (void)applyUnfocusedStyle {
    if (self.isPrimary) {
        self.backgroundColor = [ArgoTheme accent];
        [self setTitleColor:[ArgoTheme background] forState:UIControlStateNormal];
    } else {
        self.backgroundColor = [ArgoTheme surfaceRaised];
        [self setTitleColor:[ArgoTheme textPrimary] forState:UIControlStateNormal];
    }
    [self setTitleColor:[ArgoTheme textDisabled] forState:UIControlStateDisabled];
}

- (void)didUpdateFocusInContext:(UIFocusUpdateContext *)context
       withAnimationCoordinator:(UIFocusAnimationCoordinator *)coordinator {
    [super didUpdateFocusInContext:context withAnimationCoordinator:coordinator];

    BOOL gained = (context.nextFocusedView == self);
    [coordinator addCoordinatedAnimations:^{
        if (gained) {
            // 1.06배. tvOS 기본 카드 효과보다 작게 — 도구 모음이 들썩이면
            // 문서를 읽는 데 방해가 됩니다.
            self.transform = CGAffineTransformMakeScale(1.06, 1.06);
            self.backgroundColor = [ArgoTheme surfaceFocused];
            [self setTitleColor:[ArgoTheme background] forState:UIControlStateNormal];
        } else {
            self.transform = CGAffineTransformIdentity;
            [self applyUnfocusedStyle];
        }
    } completion:nil];
}

@end


#pragma mark - 표면

@implementation ArgoSurfaceView

+ (instancetype)makeSurface {
    ArgoSurfaceView *v = [[ArgoSurfaceView alloc] init];
    v.backgroundColor = [ArgoTheme surface];
    v.layer.cornerRadius = [ArgoTheme cornerRadius];
    v.layer.masksToBounds = YES;
    v.translatesAutoresizingMaskIntoConstraints = NO;
    return v;
}

@end
