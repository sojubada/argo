//
//  ArgoTheme.m
//  Argo
//

#import "ArgoTheme.h"

@implementation ArgoTheme

#pragma mark - 테마

/// 테마 여섯 개. 바탕 셋과 포인트 색 하나만 다르다 — 글자·위험 색은 같아야
/// 어느 테마에서도 경고가 경고로 읽힌다.
///
/// 순서는 설정 화면·아이콘 이름과 맞춘다:
/// 0 Midnight · 1 Ocean · 2 Forest · 3 Ember · 4 Orchid · 5 Gold
typedef struct { CGFloat bg[3], canvas[3], surface[3], raised[3], accent[3]; } ArgoPalette;

static const ArgoPalette kPalettes[6] = {
    { {0.07,0.07,0.08}, {0.13,0.13,0.15}, {0.16,0.16,0.18}, {0.24,0.24,0.27}, {0.29,0.69,0.78} },
    { {0.04,0.07,0.12}, {0.08,0.12,0.19}, {0.10,0.15,0.23}, {0.16,0.22,0.32}, {0.38,0.66,1.00} },
    { {0.05,0.09,0.07}, {0.09,0.14,0.11}, {0.11,0.17,0.13}, {0.17,0.25,0.20}, {0.40,0.82,0.55} },
    { {0.10,0.06,0.05}, {0.16,0.10,0.08}, {0.19,0.12,0.10}, {0.28,0.18,0.15}, {1.00,0.58,0.32} },
    { {0.08,0.05,0.11}, {0.13,0.09,0.17}, {0.16,0.11,0.21}, {0.24,0.17,0.30}, {0.78,0.58,1.00} },
    { {0.09,0.08,0.05}, {0.14,0.13,0.09}, {0.18,0.16,0.11}, {0.26,0.24,0.17}, {0.95,0.78,0.36} },
};

+ (NSInteger)themeIndex {
    // 설정은 Spartan 때부터 쓰던 모음에 둔다. Swift 의 UserDefaults.settings 와 같은 곳이다.
    static NSUserDefaults *settings;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        settings = [[NSUserDefaults alloc] initWithSuiteName:@"com.whitetailani.Spartan.settings"];
    });
    NSInteger i = [settings integerForKey:@"argoTheme"];
    return (i >= 0 && i < 6) ? i : 0;
}

static UIColor *ArgoRGB(const CGFloat c[3], CGFloat alpha) {
    return [UIColor colorWithRed:c[0] green:c[1] blue:c[2] alpha:alpha];
}

#pragma mark - 색

+ (UIColor *)background       { return ArgoRGB(kPalettes[[self themeIndex]].bg, 1.0); }
+ (UIColor *)canvasBackground { return ArgoRGB(kPalettes[[self themeIndex]].canvas, 1.0); }
+ (UIColor *)surface          { return ArgoRGB(kPalettes[[self themeIndex]].surface, 0.92); }
+ (UIColor *)surfaceRaised    { return ArgoRGB(kPalettes[[self themeIndex]].raised, 1.0); }
+ (UIColor *)surfaceFocused   { return [UIColor colorWithRed:0.95 green:0.95 blue:0.96 alpha:1.0]; }

+ (UIColor *)textPrimary     { return [UIColor colorWithWhite:0.97 alpha:1.0]; }
+ (UIColor *)textSecondary   { return [UIColor colorWithWhite:0.66 alpha:1.0]; }
+ (UIColor *)textDisabled    { return [UIColor colorWithWhite:0.38 alpha:1.0]; }

/// 포인트 색. 테마마다 하나.
+ (UIColor *)accent          { return ArgoRGB(kPalettes[[self themeIndex]].accent, 1.0); }
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
